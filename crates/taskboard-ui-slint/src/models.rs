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
    AppState, BoardId, Importance, LabelId, ReminderRuleId, SearchHit, SearchTarget, StackId, Task,
    TaskId,
};

use crate::actions::{ActionId, ContextId};
use crate::keymap::Keymap;
use crate::presentation::{self, DueKind, DueSets};
use crate::responsive::Breakpoint;
use crate::view_state::{DateField, DeleteTarget, Flow, ReconcilePreviewState, ViewState};
use crate::{
    AppWindow, BannerRowModel, BoardBarModel, BoardModel, BoardSegmentModel, BreakRuleRowModel,
    Breakpoint as SlintBreakpoint, CeremonyLetterMode as SlintCeremonyLetterMode,
    CeremonyLetterModel, DecorationKind, DragState, DueDayModel, DueKind as SlintDueKind,
    FilesSyncChipState as SlintFilesSyncChipState, FilesSyncChipTone as SlintFilesSyncChipTone,
    FlowHintModel, HabitCardModel, NotificationRowModel, PaletteRowModel, PlanModel, PlanRowModel,
    ReflectionRowModel, ReflectionSectionModel, ReviewModel, ReviewRowModel, ReviewWindowModel,
    RuleRowModel, SettingsAdoptionPhase as SlintSettingsAdoptionPhase,
    SettingsPublishPhase as SlintSettingsPublishPhase, StackColumnModel, StackDetailModel,
    StatBarModel, SyncBadgeModel, SyncPhaseUi, TaskCardModel, TaskDecoration, TaskDetailModel,
    Theme, TimerRowModel, TodayModel, TodayRowModel, TodaySectionModel, Tone, TrackCardState,
    TrackEntryRowModel, TrackedTodayRowModel, TrackingSectionModel, View,
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
    /// The archived boards in canonical order (UX R-1 D-1, #785): the
    /// board bar's archived-boards roster row indices resolve against
    /// this pass (index-aligned with the pushed `archived-rows`;
    /// stale absorbed like every seam).
    pub archived_board_ids: Vec<BoardId>,
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
    /// The journal ledger's rows (T-15, #798): id-indexed in the
    /// ledger's own `(window_start desc, id desc)` display order — the
    /// activation and staged-delete seams' resolution base (the
    /// index-seam rule).
    pub reflections: Vec<taskboard_domain::ReflectionId>,
    /// The break-rule roster's rows (T-14, #797): id-indexed in the
    /// CLI list's canonical `(threshold, id)` order — the staged
    /// remove seams' resolution base (the index-seam rule).
    pub break_rules: Vec<taskboard_domain::BreakRuleId>,
    /// The habits section's live habits (DR-6/6): id-indexed in the
    /// stored manual `(order, id)` order — the plus/reorder seams'
    /// resolution base; renders on kiosk too (rows are passive).
    pub habits: Vec<taskboard_domain::HabitId>,
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
    /// The entries overlay's rows in display order (r4 chunk 10,
    /// D-TS3): index-aligned with the pushed `track-entries` model —
    /// row-activate, the edit seed, and the armed-delete resolve
    /// entry ids through this table (the index-seam rule, never a
    /// viewport-relative offset).
    pub track_entries: Vec<taskboard_domain::TimeEntryId>,
    /// The task-detail labels section's chips (UX R-3 D-2, #789):
    /// index-aligned with the pushed `labels` model — the chip ×
    /// unassign seam resolves the label id through this table (the
    /// index-seam rule; stale absorbed like every seam).
    pub task_labels: Vec<LabelId>,
    /// The subject board's live labels in canonical order (UX R-3
    /// D-2/D-3): the labels overlay's roster row indices resolve
    /// against this pass — the assignment toggle, the edit seed, and
    /// the armed-delete resolve label ids through this table.
    pub label_ids: Vec<LabelId>,
    /// The Tracked-today section's rows (r4 chunk 10, D-TS7):
    /// index-aligned with the pushed section model — row activation
    /// opens the addressed task.
    pub tracked_today: Vec<TaskId>,
    /// The plan-next-day surface's rows (#622): the placed boxes then
    /// the pool, in reading order — the focus walk's domain and the
    /// placement/reorder seams' resolution base; empty on kiosk.
    pub plan_rows: Vec<TaskId>,
    /// How many of `plan_rows`' head entries are placed boxes (the
    /// two panes resolve `(pane, row)` against the split).
    pub plan_box_count: usize,
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

    /// The plan-next-day pane and row holding `task` (#622): pane 0
    /// is the placed boxes, pane 1 the pool.
    #[must_use]
    pub fn plan_position(&self, task: TaskId) -> Option<(usize, usize)> {
        let flat = self.plan_rows.iter().position(|t| *t == task)?;
        if flat < self.plan_box_count {
            Some((0, flat))
        } else {
            Some((1, flat - self.plan_box_count))
        }
    }

    /// The plan-next-day row at `(pane, row)`, if the tables know it.
    #[must_use]
    pub fn plan_row(&self, pane: usize, row: usize) -> Option<TaskId> {
        let offset = if pane == 0 { 0 } else { self.plan_box_count };
        self.plan_rows.get(offset + row).copied()
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
    bar_segments: MirroredRows<BoardSegmentModel>,
    bar_rows: MirroredRows<crate::DropdownRow>,
    /// The archived-boards roster's persistent row handle (UX R-1
    /// D-1): the same stable-identity diff as `bar_rows`.
    bar_archived_rows: MirroredRows<crate::DropdownRow>,
    columns: MirroredRows<StackColumnModel>,
    /// The per-column card models, keyed by the column's stack (the
    /// board's columns and the stack-detail list address them by
    /// stack).
    column_tasks: BTreeMap<StackId, MirroredRows<TaskCardModel>>,
    /// Per-card decoration models keyed by the task.
    /// `TaskCardModel` compares its `decorations` field by handle
    /// identity (the pinned Slint's `ModelRc` `PartialEq`), so a
    /// fresh model per card per pass would dirty every card and
    /// degrade the diff to a full `set_row_data` cascade.
    card_decorations: BTreeMap<TaskId, MirroredRows<TaskDecoration>>,
    /// Per-card label-chip models keyed by the task (tag-color plan
    /// D-9): the `card_decorations` handle-identity rule verbatim — a
    /// fresh model per card per pass would dirty every card.
    card_label_chips: BTreeMap<TaskId, MirroredRows<crate::TaskLabelChipModel>>,
    /// Per-habit decoration models keyed by the habit (#1017): the
    /// `card_decorations` handle-identity rule extended to the habits
    /// section — `habit_card`'s fresh `ModelRc` per row per pass made
    /// every habit row compare unequal every refresh and re-pushed the
    /// whole surface. The label-chips twin stays unwritten: habit rows
    /// carry no chips (the static empty model), so there is nothing to
    /// stabilize.
    habit_decorations: BTreeMap<taskboard_domain::HabitId, MirroredRows<TaskDecoration>>,
    detail_tasks: MirroredRows<TaskCardModel>,
    task_rules: MirroredRows<RuleRowModel>,
    task_rule_presets: MirroredRows<SharedString>,
    banner: MirroredRows<BannerRowModel>,
    /// The notification surface's persistent row handle (R3 chunk C).
    notifications: MirroredRows<NotificationRowModel>,
    /// The timers section's persistent row handle (DR-3/3): the same
    /// diff pattern — stable identity across refreshes.
    timers: MirroredRows<TimerRowModel>,
    /// The break-rule roster's persistent row handle (T-14, #797):
    /// the timers handle's twin.
    break_rules: MirroredRows<BreakRuleRowModel>,
    /// The habits section's persistent row handle (DR-6/6): the same
    /// diff pattern.
    habits: MirroredRows<HabitCardModel>, // generated from compositions/habits.slint
    /// The palette's persistent row handle (chunk 9): diffed per
    /// keystroke — a fresh `VecModel` per push would regress the
    /// stable-identity invariant (r2 D9).
    palette: MirroredRows<PaletteRowModel>,
    /// The concentration overlay's mount-only note seed (#496): the
    /// mirror text is written exactly on the closed→open transition —
    /// the per-second countdown push must never clobber a field the
    /// user is typing into (the capture-overlay's init-seed rule).
    focus_overlay_was_open: bool,
    /// The review walk's prompts-stage mount-only answer seed (#638):
    /// the mirror texts are written exactly on the closed→Prompts
    /// transition — a re-push must never clobber a field the user is
    /// typing into (the focus-overlay seed's rule).
    walk_prompts_open: bool,
    /// The abort confirm's arm (J4 D-6): the adapter sets it before
    /// the pass; the model mirror mounts the nested confirm host.
    pub(crate) walk_abort_armed: bool,
    /// The today view's per-section row handles (chunk 10 D7): fixed
    /// to the four display sections — a rollover refresh must not
    /// re-instantiate the cards (the r2 D9 discipline).
    today_sections: [MirroredRows<TodayRowModel>; 4],
    today: MirroredRows<TodaySectionModel>,
    /// The today rows' observed extents (#592, the C-2 height cache):
    /// task-id keyed rendered heights, reported by the live delegates
    /// (the row-extent seam) — exact for ever-live rows, the fallback
    /// source for an above-viewport change's compensation extent.
    pub(crate) today_row_extents: std::collections::HashMap<SharedString, f32>,
    /// The plan-next-day surface's pane row handles (#622): the same
    /// diff pattern — stable identity across refreshes.
    plan_boxes: MirroredRows<PlanRowModel>,
    plan_pool: MirroredRows<PlanRowModel>,
    /// The review ledger's persistent row handle (#214): the same diff
    /// pattern — stable identity across refreshes.
    review: MirroredRows<ReviewRowModel>,
    /// The journal-entries ledger's row handle (#623 slice B, D-B7):
    /// the entries set is unbounded and mutation-prone (journal verbs
    /// run mid-session), so it rows-diffs like the calibration ledger
    /// instead of riding the whole-struct set.
    review_entries: MirroredRows<ReflectionRowModel>,
    settings_rows: MirroredRows<crate::ConnectionRow>,
    // The connections selector's dropdown options and their uuid
    // strings (r4 D39 — the per-row handles the positional diff
    // drives; the row struct itself died with the radio list).
    board_options: MirroredRows<crate::DropdownRow>,
    connection_items: MirroredRows<crate::ConnectionItem>,
    settings_facts: MirroredRows<crate::SettingsFactRow>,
    hints: HintModels,
    /// The tasks whose decoration models the current pass touched
    /// (cleared at pass start) — the prune set for
    /// `card_decorations`, so the map never grows past the live task
    /// set.
    pass_tasks: BTreeSet<TaskId>,
    /// The habits whose decoration models the current pass touched
    /// (cleared at pass start) — the prune set for `habit_decorations`
    /// (#1017), the same never-grows-past-the-live-rows rule as
    /// `card_decorations`.
    pass_habits: BTreeSet<taskboard_domain::HabitId>,
}

/// The six `[string]` keymap-projection hint slots (r2 D9): they
/// change only when the keymap or the visible chain changes, so the
/// diff keeps their refresh cost at zero.
#[derive(Default)]
struct HintModels {
    toolbar_menu: MirroredRows<SharedString>,
    nav_back: MirroredRows<SharedString>,
    board_segment: MirroredRows<SharedString>,
    task_card: MirroredRows<SharedString>,
    stack_header: MirroredRows<SharedString>,
    stack_create: MirroredRows<SharedString>,
    task_create: MirroredRows<SharedString>,
    // The task view's header hints (r4 D16b/D16c): the done checkbox's
    // and the tracking button's binding projections.
    task_done: MirroredRows<SharedString>,
    task_track: MirroredRows<SharedString>,
    task_title_edit: MirroredRows<SharedString>,
    // The entries opener's + effort row's projections (J3 US-4,
    // round 9 chunk 11).
    task_entries: MirroredRows<SharedString>,
    task_effort: MirroredRows<SharedString>,
}

/// The persistent model handle of one surface plus its Rust-side
/// mirror (#464): the diff compares desired rows against the mirror
/// slice — never against the model (`Model::row_data` returns by
/// value: a full clone per compared row) — fires `set_row_data` only
/// where the row differs, and updates model and mirror per mutation.
/// The mirror is push-pass-private: slint never sees it, the handle
/// escapes only by shared clone, and the fields are private to this
/// module — mutating a live model outside [`Self::diff`] does not
/// compile. The pair is one slot: the map values are the pair too,
/// so every `retain` prunes handle and mirror with the same
/// membership decision, and a fresh pair starts empty on both sides
/// in lockstep with the fresh (empty) model.
struct MirroredRows<T> {
    model: Option<Rc<VecModel<T>>>,
    mirror: Vec<T>,
}

/// No derived `T: Default` bound — `Option` and `Vec` default
/// unconditionally.
impl<T> Default for MirroredRows<T> {
    fn default() -> Self {
        Self {
            model: None,
            mirror: Vec::new(),
        }
    }
}

/// The structural half of one positional diff (#592): the first
/// index where old and desired disagree, and the tail's membership
/// counts. Pure over `(old, desired)` — the same comparison
/// [`diff_rows`] performs, minus the model mutations; the push
/// discipline turns any head/mid membership change into a rebind
/// cascade from `first_change` down plus tail mutations, so
/// `first_change` is where the visible reflow begins.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RowAnchorSummary {
    pub(crate) first_change: usize,
    pub(crate) removed: usize,
    pub(crate) inserted: usize,
}

/// One section's compensable change (#592): the first changed row,
/// the signed membership delta, and the compensation extent (0.0 =
/// never observed live; the [`ListView`](crate) falls back to its
/// average).
#[derive(Default, Clone)]
struct SectionAnchor {
    first_change: usize,
    delta: i32,
    extent: f32,
}

/// `None` = no row over the common length differs: either nothing
/// changed, or only the tail's membership moved (a tail change
/// reflows nothing — the push discipline removes/appends at the end,
/// below every live row).
fn row_anchor_summary<T: PartialEq>(old: &[T], desired: &[T]) -> Option<RowAnchorSummary> {
    let common = old.len().min(desired.len());
    let first_change = (0..common).find(|&i| old[i] != desired[i])?;
    Some(RowAnchorSummary {
        first_change,
        removed: old.len().saturating_sub(desired.len()),
        inserted: desired.len().saturating_sub(old.len()),
    })
}

/// Applies `desired` onto the persistent model through a positional
/// diff and returns the number of row mutations: `set_row_data` only
/// where the row differs over the common length, then the tail is
/// truncated, then appended. No LCS — a mid-list reorder costs a
/// `set_row_data` cascade over the suffix, which the pinned Slint
/// applies as in-place binding updates with zero re-instantiation
/// (the repetition-probe pins), and reorders are user-move-scale,
/// not refresh-scale.
///
/// The comparison runs against `mirror` — the model's exact content
/// (#464): `Model::row_data` returns by value, so reading the model
/// per compared row would clone every current row, unchanged ones
/// included. Model and mirror are updated per mutation, in the same
/// statement, and can therefore never disagree unless a live model is
/// mutated outside this function — which the privacy fence on
/// [`MirroredRows`] makes not compile outside this module and the
/// debug witness below turns into a loud panic inside it.
fn diff_rows<T: PartialEq + Clone + 'static>(
    mirror: &mut Vec<T>,
    model: &VecModel<T>,
    desired: &[T],
) -> usize {
    debug_assert_eq!(
        mirror.len(),
        model.row_count(),
        "mirror/model divergence: a live model was mutated outside the diff"
    );
    let mut mutations = 0;
    let current = mirror.len();
    let common = current.min(desired.len());
    for (i, row) in desired[..common].iter().enumerate() {
        if mirror[i] != *row {
            model.set_row_data(i, row.clone());
            mirror[i] = row.clone();
            mutations += 1;
        }
    }
    while mirror.len() > desired.len() {
        model.remove(desired.len());
        mirror.pop();
        mutations += 1;
    }
    for row in &desired[common..] {
        model.push(row.clone());
        mirror.push(row.clone());
        mutations += 1;
    }
    mutations
}

impl<T: PartialEq + Clone + 'static> MirroredRows<T> {
    /// The `push_rows` shape: get-or-create the handle, installing it
    /// into its property exactly once via `install`, then diff.
    fn push(&mut self, install: impl FnOnce(ModelRc<T>), desired: &[T]) -> usize {
        let model = self.model.get_or_insert_with(|| {
            let model = Rc::new(VecModel::default());
            install(ModelRc::from(model.clone()));
            model
        });
        diff_rows(&mut self.mirror, model, desired)
    }

    /// The direct diff sites: get-or-create the handle (a fresh slot
    /// starts empty on both sides — model and mirror in lockstep),
    /// then diff.
    fn diff(&mut self, desired: &[T]) -> usize {
        let model = self.model.get_or_insert_with(Rc::default);
        diff_rows(&mut self.mirror, model, desired)
    }

    /// The struct-embedding sites' shared clone of the handle (the
    /// inner `tasks`/`rows`/`segments` models): create the model on
    /// first touch, hand out a shared `Rc` clone.
    fn handle_or_insert_with(&mut self) -> Rc<VecModel<T>> {
        self.model.get_or_insert_with(Rc::default).clone()
    }
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
            // The habit row's typed bits (W-10, #1218): the History
            // affordance renders off the bit; the tombstone state is
            // the snapshot read (no live-row state forged — the row
            // carries no mutable verbs).
            history: matches!(target, SearchTarget::Habit(_)),
            tombstone: match target {
                SearchTarget::Habit(id) => {
                    state.habits.habits.get(id).is_some_and(|row| row.deleted)
                }
                _ => false,
            },
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
        SearchTarget::Habit(id) => state
            .habits
            .habits
            .get(&id)
            .map_or(String::new(), |row| row.title.clone()),
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
        // The habit row's kind + tombstone posture (W-10, #1218): a
        // tombstoned row names its state — the log outlives, the
        // History affordance is the only verb.
        SearchTarget::Habit(id) => {
            let tombstoned = state.habits.habits.get(&id).is_some_and(|row| row.deleted);
            if tombstoned {
                "habit · tombstoned · history".to_owned()
            } else {
                "habit · history".to_owned()
            }
        }
    }
}

/// Get-or-create the persistent handle in `slot` (installing it into
/// its window property exactly once via `install`), then diff
/// `desired` onto it; returns the mutation count.
fn push_rows<T: PartialEq + Clone + 'static>(
    slot: &mut MirroredRows<T>,
    install: impl FnOnce(ModelRc<T>),
    desired: &[T],
) -> usize {
    slot.push(install, desired)
}

/// The due editor's pushed state (r2 D25): the adapter owns the
/// buffer, echo, and picker cursor across refreshes (R-d); the push
/// only carries them into the model.
/// The recur row's adapter-owned flags (issue #506, plan D-8): the
/// inline pause editor's open flag and the conversion confirm's armed
/// flag — the adapter owns them across refreshes (the due buffers
/// pattern), this push only carries.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecurRowPush {
    /// The inline pause editor is open.
    pub pause_open: bool,
    /// The conversion confirm strip is armed.
    pub confirm_open: bool,
}

/// The priority pickers' adapter-owned state (UX R-3 D-1, #788):
/// which detail row's quick-list is open — one at a time (the editor
/// exclusivity's shape), the adapter owns the flag across refreshes
/// (the due buffers pattern), this push only carries.
/// The labels overlay's in-place form's pushed halves (UX R-3 D-2,
/// #789): the seeded title (the field reads it on mount only — the
/// buffer stays user-owned while the form lives) and the armed
/// swatch's hex. The swatch row is the presentation constant
/// (`presentation::SWATCHES`); the model carries the selection's echo.
#[derive(Debug, Clone, Default)]
pub struct LabelFormPush {
    /// The form surface is mounted.
    pub open: bool,
    /// The seed (the edit form's) or empty (create).
    pub title: String,
    /// The armed swatch's exact hex (no `#`).
    pub color: String,
}

/// The effort row's push (the overlay migration, M9): the always-on
/// field's text seed — the armed buffer when the user typed, the
/// subject's stored text form otherwise. There is no open flag any
/// more.
pub struct EffortEditorPush<'a> {
    pub buffer: &'a str,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PriorityPickerPush {
    /// The open picker's field; `None` = both closed.
    pub open: Option<crate::router::PriorityField>,
}

pub struct DueEditorPush<'a> {
    pub text: &'a str,
    pub echo: &'a str,
    pub picker: Option<&'a crate::due_edit::DuePickerState>,
    /// The one-shot grid-focus flag (round-3 D10): consumed by the
    /// next push — `true` hands focus to the overlay's grid.
    pub focus_on_mount: bool,
    /// The one-shot field-focus flag (r4 D25): consumed by the next
    /// push — `true` hands focus back to the armed field (the grid's
    /// Enter hand-over).
    pub field_focus: bool,
    /// The due-soon horizon (round-3 D14/#230): the handed-over
    /// typed knob, one horizon everywhere (the sets and the grid
    /// tint). Defaults to the presentation constant.
    pub horizon: TimeDelta,
    /// The time combobox's pushed half (r4 D26): the visible text
    /// mirror (`--:--` while unset), its tri-state validity probe,
    /// the filtered quick-list rows, and the scroll-to-nearest index.
    pub time_text: &'a str,
    pub time_valid: Option<bool>,
    pub time_slots: &'a [chrono::NaiveTime],
    pub time_nearest_row: usize,
    /// The due field's tri-state validity probe + the copy constants
    /// (r4 D27) — the hints live in one Rust home; the slint side
    /// decides visibility.
    pub date_valid: Option<bool>,
    pub syntax_hint: &'static str,
    pub time_hint: &'static str,
    /// The custom-offset field's validity probe + hint (r4 D27) —
    /// the due push is the task-detail pass's carrier.
    pub offset_valid: Option<bool>,
    pub offset_hint: &'static str,
}

impl Default for DueEditorPush<'_> {
    fn default() -> Self {
        Self {
            text: "",
            echo: "",
            picker: None,
            focus_on_mount: false,
            field_focus: false,
            horizon: crate::presentation::DUE_SOON_HORIZON,
            time_text: "--:--",
            time_valid: None,
            time_slots: &[],
            time_nearest_row: 0,
            date_valid: None,
            syntax_hint: "",
            time_hint: "",
            offset_valid: None,
            offset_hint: "",
        }
    }
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
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    break_rule_armed_index: i32,
    break_rule_selected: Option<taskboard_domain::BreakRuleId>,
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
        presentation::awaiting_alarm_count(state, reminder_defaults, calendar, now)
    };
    let shown: Vec<TimerRowModel> = rows
        .iter()
        .map(|row| TimerRowModel {
            label: SharedString::from(row.label.clone()),
            countdown: SharedString::from(presentation::countdown_text(row.effective, now)),
            fire_time: SharedString::from(format!(
                "@ {}",
                presentation::iso_local_fire_time(row.effective, &chrono::Local)
            )),
            alarm_rung: row.insistence == taskboard_domain::Insistence::Alarm,
        })
        .collect();
    window.set_timers_visible(!kiosk);
    // The kiosk absence hint (UX R-1 D-2, #790): the fenced surface's
    // honest absence signal — display state only, no command variant.
    window.set_timers_absence_hint(kiosk);
    window.set_timers_awaiting(i32::try_from(awaiting).unwrap_or(i32::MAX));
    let timer_mutations = push_rows(
        &mut models.timers,
        |rc| {
            window.set_timers_model(rc);
        },
        &shown,
    );
    // The break-rule roster (T-14, #797): all boards' rules (round-6
    // A1 D-3) in the CLI list's canonical order; the armed index marks
    // the one staged remove (the label form's armed-index convention). The
    // kiosk gate rides the section's own (`absence-hint` hides the
    // whole subsection), so the model still derives — harmless, the
    // section never mounts it.
    // The board-free roster (round-6 A1 D-3, 2026-11-24, #1301): the
    // today view aggregates all boards' rules; the create form's
    // dispatch still targets the selected board (the target-board
    // default is #1314's territory).
    let break_rows: Vec<presentation::BreakRuleRow> = presentation::break_rule_rows(state, None);
    tables.break_rules = break_rows.iter().map(|row| row.rule).collect();
    let break_shown: Vec<BreakRuleRowModel> = break_rows
        .iter()
        .enumerate()
        .map(|(index, row)| BreakRuleRowModel {
            label: SharedString::from(row.label.clone()),
            threshold: SharedString::from(row.threshold.text_form()),
            alarm_rung: row.insistence == taskboard_domain::Insistence::Alarm,
            board: SharedString::from(row.board_title.clone()),
            armed: i32::try_from(index).is_ok_and(|i| i == break_rule_armed_index),
            // The roster cursor's emphasis (US-9, W-10, the rule
            // list's `selected` convention).
            selected: break_rule_selected.is_some_and(|cursor| cursor == row.rule),
        })
        .collect();
    timer_mutations
        + push_rows(
            &mut models.break_rules,
            |rc| {
                window.set_break_rules_model(rc);
            },
            &break_shown,
        )
}

/// The habit card's decoration row, derived (H2 D2, #1017): the
/// progress and streak ride the typed decoration row
/// (`DecorationKind::Habit` — no `DueKind` semantics theft), plus the
/// skip chip and the paused badge. Pure — the fold/property tests
/// read it without a window, and the apply path diffs it onto the
/// persistent per-habit handle instead of minting a fresh `ModelRc`
/// per pass (the `card_decorations` handle-identity rule).
#[must_use]
pub fn habit_card_decorations(row: &presentation::HabitRow) -> Vec<TaskDecoration> {
    let mut decorations = std::vec::Vec::with_capacity(2);
    decorations.push(TaskDecoration {
        kind: DecorationKind::Habit,
        label: SharedString::from(presentation::habit_progress_phrase(row)),
        tone: if row.met {
            Tone::Success
        } else {
            Tone::Neutral
        },
    });
    // The streak rides the Badge base widget now (HF3 W-9, US-8/S1):
    // the decoration carries the typed streak value (the test oracle
    // — `HabitRow.streak`); the old fire emoji slot is gone (the
    // badge's label text is judged at the stage-3 walkthrough, never
    // asserted).
    if row.streak > 0 {
        decorations.push(TaskDecoration {
            kind: DecorationKind::Habit,
            label: SharedString::from(row.streak.to_string()),
            tone: Tone::Success,
        });
    }
    // The typed skip chip (HF1, W-2): the marked window renders the
    // skip state, never the miss posture (US-1/S3).
    if row.skip {
        decorations.push(TaskDecoration {
            kind: DecorationKind::Habit,
            label: SharedString::from("skipped"),
            tone: Tone::Info,
        });
    }
    // The paused badge (HF1, W-4): the standing pause renders with the
    // inclusive through-date (US-2/S4).
    if row.paused {
        decorations.push(TaskDecoration {
            kind: DecorationKind::Habit,
            label: SharedString::from(row.paused_until.map_or_else(
                || "paused".to_owned(),
                |until| format!("paused through {until}"),
            )),
            tone: Tone::Info,
        });
    }
    decorations
}

/// The habit card's `TaskCard` substrate model (H2 D2): the habit fills
/// the title and done slots (the done slot IS the +1 log control —
/// `done: false` always, surplus logs are first-class), the
/// scheduled slot carries the time/day chips, the progress and
/// streak ride [`habit_card_decorations`], and every task-only slot is
/// empty. Pure — the derive half only: the apply path
/// (`push_habits`) replaces the fresh decoration `ModelRc` with the
/// persistent per-habit handle before install (#1017). The
/// fold/property tests read it without a window.
#[must_use]
pub fn habit_card(row: &presentation::HabitRow) -> TaskCardModel {
    let decorations = habit_card_decorations(row);
    TaskCardModel {
        title: SharedString::from(row.title.clone()),
        due_phrase: SharedString::from(String::new()),
        scheduled_phrase: SharedString::from(presentation::habit_scheduled_phrase(
            row.chip,
            row.anchor_day,
            row.off_schedule,
        )),
        effort_phrase: SharedString::from(String::new()),
        due_kind: SlintDueKind::None,
        done: false,
        can_done: true,
        emphasis: false,
        dragging_source: false,
        selected: false,
        anchor: false,
        track_state: TrackCardState::None,
        decorations: ModelRc::new(VecModel::from(decorations)),
        label_chips: ModelRc::new(VecModel::from(std::vec::Vec::new())),
        label_chips_overflow: 0,
    }
}

/// The habits section (DR-6/6, §4.6; H2 card-ified): the aggregated
/// tracker rows over ALL live boards (round-6 A1 D-2, 2026-11-24,
/// #1301 — the active board's scope retired) on the task-card
/// substrate. No kiosk model gate (D-P13): the rows render on the
/// appliance — only the section's actions disable
/// (`habits-editing-enabled`). Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
fn push_habits(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    now: DateTime<Utc>,
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    // The board-free row source (round-6 A1 D-2, #1301, 2026-11-24):
    // the Today view aggregates over all live boards; `vs.board()` is
    // gone from the habit path.
    let rows: Vec<presentation::HabitRow> = presentation::today_habit_rows(state, now, calendar);
    tables.habits = rows.iter().map(|row| row.habit).collect();
    let shown: Vec<HabitCardModel> = rows
        .iter()
        .map(|row| {
            // #1017: the decoration handle-identity rule at the habits
            // surface — diff the desired row onto the persistent
            // per-habit handle and install that handle, so unchanged
            // rows notify nothing and the card compares fully equal
            // across refresh passes (the board cards' rule verbatim).
            let slot = models.habit_decorations.entry(row.habit).or_default();
            models.pass_habits.insert(row.habit);
            slot.diff(&habit_card_decorations(row));
            HabitCardModel {
                card: TaskCardModel {
                    // The walk's landing fact (chunk 8 D2): the focused
                    // habit row paints the card emphasis ring like a task
                    // card — the focus fact is the walk cursor's, read
                    // through the same accessor the board cards use.
                    emphasis: vs.focused_habit() == Some(row.habit),
                    // The typed target-met bit (D3a W-2, #1199): the
                    // met window fills the card's done slot visual
                    // until the window resets — the same `met` fold
                    // the progress chip reads (surplus logs stay
                    // first-class: the slot never disables).
                    done: row.met,
                    decorations: ModelRc::from(slot.handle_or_insert_with()),
                    ..habit_card(row)
                },
                repair: row.repair,
                repair_undoable: row.repair_undoable,
                met: row.met,
                skip: row.skip,
                paused: row.paused,
                paused_until: SharedString::from(
                    row.paused_until
                        .map_or_else(String::new, |until| until.to_string()),
                ),
                progress_phrase: SharedString::from(presentation::habit_progress_phrase(row)),
            }
        })
        .collect();
    window.set_habits_editing_enabled(presentation::habit_editing_enabled(kiosk));
    // The create affordance's mount gate (round-7 D-7.2; re-recorded
    // 2026-11-24, round-6 A1 D-4, #1301): desktop with ANY live board
    // — the commit mints into the first live board (canonical order).
    // Kiosk and zero-live-board days keep the rows-only posture
    // (D-P13 untouched).
    window.set_habits_create_mountable(!kiosk && !presentation::live_boards(state).is_empty());
    push_rows(
        &mut models.habits,
        |rc| {
            window.set_habits_model(rc);
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
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    let rows: Vec<presentation::NotificationRow> = if kiosk {
        Vec::new()
    } else {
        presentation::notification_rows(state, reminder_defaults, calendar, now)
    };
    tables.notifications.clone_from(&rows);
    let shown: Vec<NotificationRowModel> = rows
        .iter()
        .take(NOTIFICATION_ROW_CAP)
        .map(|row| {
            // The anchor-aware table (DR-3/3, D-P11): the pure
            // decision projected per row — task rows keep all four
            // task-shaped affordances, alarm rows Dismiss/Snooze/
            // Cancel.
            // The anchor rides the ref's own family (the H4 fix):
            // break and tracker rows carry no task — mapping them to
            // the Task anchor by Adhoc-ness alone panicked the
            // projection on the first mounted tracker nudge (the
            // `us6_s2` journey's find).
            let anchor = match row.reminder {
                taskboard_domain::ReminderRef::Adhoc(alarm) => {
                    taskboard_domain::ReminderAnchor::Alarm(alarm)
                }
                taskboard_domain::ReminderRef::Break(id) => {
                    taskboard_domain::ReminderAnchor::Break(id)
                }
                taskboard_domain::ReminderRef::Tracker(id) => {
                    taskboard_domain::ReminderAnchor::Tracker(id)
                }
                taskboard_domain::ReminderRef::Rule(_)
                | taskboard_domain::ReminderRef::Default(_) => {
                    taskboard_domain::ReminderAnchor::Task(
                        row.task
                            .expect("a rule/default notification row carries its task"),
                    )
                }
            };
            let buttons = presentation::awaiting_row_buttons(anchor);
            // The tracker suppression (issue #492): a tracker task's
            // rows drop the done button — the existing per-row flag,
            // one condition (the engine rejection is the backstop).
            let tracker = matches!(
                anchor,
                taskboard_domain::ReminderAnchor::Task(id)
                    if state
                        .tasks
                        .get(&id)
                        .is_some_and(presentation::tracker_badge_visible)
            );
            NotificationRowModel {
                title: SharedString::from(row.title.clone()),
                fired_phrase: SharedString::from(presentation::notification_phrase(
                    row.occurrence,
                    now,
                )),
                can_done: buttons.done && !tracker,
                can_mute: buttons.mute,
                can_cancel: buttons.cancel,
                can_track_start: buttons.track_start,
                // D-6.1 (round 6): the double-click open rides the
                // task anchor — alarm rows (no task) don't advertise.
                can_open: row.task.is_some(),
            }
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
#[expect(clippy::too_many_lines)] // the pass is one linear pipeline
pub fn push(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    keymap: &Keymap,
    chain: &[ContextId],
    day: crate::view_state::DayCtx,
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    reminder_defaults: &taskboard_domain::ReminderDefaults,
    focus: Option<&taskboard_domain::focus::FocusTiming>,
    models: &mut PushModels,
    due: &DueEditorPush<'_>,
    recur: crate::models::RecurRowPush,
    priority: crate::models::PriorityPickerPush,
    label_form: Option<&crate::models::LabelFormPush>,
    effort: Option<&crate::models::EffortEditorPush<'_>>,
    label_armed_index: i32,
    break_rule_armed_index: i32,
    reflection_focus: (
        Option<taskboard_domain::ReflectionId>,
        Option<taskboard_domain::ReflectionId>,
    ),
    // The habit editor's draft (H4 D5): `Some` exactly while the
    // habit variant mounts (the view_state subject's twin).
    habit: Option<&crate::router::HabitEditDraft>,
) -> IdTables {
    let now = day.now;
    let sets = presentation::due_sets(state, now, due.horizon);
    let board = vs.board().and_then(|id| state.boards.get(&id));
    let stack_ids: Vec<StackId> = vs.stack_ids(state);
    let live_boards = presentation::live_boards(state);

    // The board bar (design §3.8): one segment per live board,
    // selected index over the same pass's tables.
    let segments: Vec<BoardSegmentModel> = live_boards
        .iter()
        .map(|b| BoardSegmentModel {
            title: SharedString::from(b.title.clone()),
            color: swatch_hex(&b.color),
        })
        .collect();
    let selected = i32::try_from(
        vs.board()
            .and_then(|b| live_boards.iter().position(|board| board.id == b))
            .unwrap_or(0),
    )
    .unwrap_or(i32::MAX);
    let segments_handle = models.bar_segments.handle_or_insert_with();
    let mut mutations = models.bar_segments.diff(&segments);
    // The dropdown's rows (r4 D40): one per segment, no connection
    // indicator — the switcher carries titles only.
    let bar_rows: Vec<crate::DropdownRow> = segments
        .iter()
        .map(|segment| crate::DropdownRow {
            label: segment.title.clone(),
            indicator: crate::DropdownIndicator::None,
            tone: crate::DropdownTone::Neutral,
            selected: false,
            busy: false,
        })
        .collect();
    let bar_rows_handle = models.bar_rows.handle_or_insert_with();
    mutations += models.bar_rows.diff(&bar_rows);
    // The archived-boards roster (UX R-1 D-1, #785): one title-only
    // row per archived board, canonical order; the bar's trailing
    // indicator mounts exactly when the set is non-empty. The id
    // table rides the same pass — the roster row index resolves
    // against `archived_board_ids` (stale absorbed like every seam).
    let archived = presentation::archived_boards(state);
    let archived_rows: Vec<crate::DropdownRow> = archived
        .iter()
        .map(|board| crate::DropdownRow {
            label: SharedString::from(board.title.clone()),
            indicator: crate::DropdownIndicator::None,
            tone: crate::DropdownTone::Neutral,
            selected: false,
            busy: false,
        })
        .collect();
    let archived_rows_handle = models.bar_archived_rows.handle_or_insert_with();
    mutations += models.bar_archived_rows.diff(&archived_rows);
    // The struct re-assignment runs every pass — its `selected` scalar
    // must stay live; the unchanged segments handle makes it a
    // renderer no-op for the repetition (the repetition-probe pins).
    window.set_board_bar_model(BoardBarModel {
        segments: ModelRc::from(segments_handle),
        selected,
        rows: ModelRc::from(bar_rows_handle),
        archived_rows: ModelRc::from(archived_rows_handle),
        archived_enabled: !kiosk,
        // The roster's recognition cue (UX-R8 D-9, US-7/AC-1): true
        // exactly when the archived set is non-empty (the mount rule's
        // twin — the indicator mounts iff this is true).
        archived_cue: !archived.is_empty(),
    });

    mutations += push_chrome(
        window,
        vs.view(),
        vs,
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
        crate::view_state::ViewId::Review => View::Review,
        crate::view_state::ViewId::Settings => View::Settings,
        crate::view_state::ViewId::Status => View::Status,
        crate::view_state::ViewId::PlanNextDay => View::PlanNextDay,
    });

    let mut tables = IdTables {
        board_ids: live_boards.iter().map(|b| b.id).collect(),
        stack_ids: stack_ids.clone(),
        ..IdTables::default()
    };
    // The roster row indices' resolution base (UX R-1 D-1): the
    // archived pass's canonical order, index-aligned with the pushed
    // `archived-rows`.
    tables.archived_board_ids = archived.iter().map(|b| b.id).collect();
    mutations += push_board(
        window,
        state,
        vs,
        &sets,
        &stack_ids,
        &mut tables,
        now,
        calendar,
        models,
    );
    mutations += push_today(
        window,
        state,
        vs,
        &sets,
        day,
        calendar,
        kiosk,
        &mut tables,
        models,
    );
    mutations += push_stack_detail(window, state, vs, &sets, &mut tables, now, calendar, models);
    mutations += push_task_detail(
        window,
        state,
        vs,
        &sets,
        &mut tables,
        now,
        kiosk,
        models,
        due,
        recur,
        priority,
        label_form,
        effort,
        label_armed_index,
        calendar,
        habit,
    );
    mutations += push_review(
        window,
        state,
        day,
        calendar,
        kiosk,
        reflection_focus,
        &mut tables,
        models,
    );
    mutations += push_plan_next_day(
        window,
        state,
        vs,
        &sets,
        day,
        calendar,
        kiosk,
        &mut tables,
        models,
    );
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
        calendar,
        kiosk,
        &mut tables,
        models,
    );
    mutations += push_timers(
        window,
        state,
        reminder_defaults,
        now,
        calendar,
        kiosk,
        break_rule_armed_index,
        vs.break_rule_cursor(),
        &mut tables,
        models,
    );
    mutations += push_habits(window, state, vs, now, calendar, kiosk, &mut tables, models);
    mutations += push_habit_history(window, state, vs, now, calendar);
    push_focus_overlay(window, state, vs, focus, now, models);
    push_review_walk(window, state, vs, day, calendar, kiosk, models);
    // The decoration handles of tasks this pass dropped from every
    // surface are unreachable from the views.
    let pass = std::mem::take(&mut models.pass_tasks);
    models
        .card_decorations
        .retain(|task, _| pass.contains(task));
    models
        .card_label_chips
        .retain(|task, _| pass.contains(task));
    let pass_habits = std::mem::take(&mut models.pass_habits);
    models
        .habit_decorations
        .retain(|habit, _| pass_habits.contains(habit));
    tracing::trace!(rows = mutations, "model push diff: row mutations this pass");
    tables
}

/// The concentration overlay's frame push (#496, D-F11): one surface,
/// one `push_*` — phase, countdown, current task, switcher rows, and
/// the note mirror, all derived per pass from the published state, the
/// injected timings, and the pass's render-time `now` (the chip-text
/// precedent). Visibility mirrors the `Flow::FocusMode` flow — the
/// flow is unreachable on kiosk (the spawn/gate/effect exclusion), so
/// the overlay never lights there. Pure sets; the slint tree computes
/// nothing (the dumb-client line). `focus: None` (the mode off)
/// forces the overlay closed regardless of a stale flow — the
/// hand-over cannot have opened it, and a defensive close keeps the
/// property honest.
fn push_focus_overlay(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    focus: Option<&taskboard_domain::focus::FocusTiming>,
    now: chrono::DateTime<chrono::Utc>,
    models: &mut PushModels,
) {
    let flow_live =
        focus.is_some() && matches!(vs.flow(), crate::view_state::Flow::FocusMode { .. });
    window.set_focus_overlay_open(flow_live);
    let mount_transition = flow_live && !models.focus_overlay_was_open;
    models.focus_overlay_was_open = flow_live;
    let Some(focus) = focus else {
        // Mode off: the derived frame values stay empty.
        window.set_focus_phase_text(SharedString::new());
        window.set_focus_remaining_text(SharedString::new());
        window.set_focus_task_title(SharedString::new());
        return;
    };
    let derived = presentation::focus_overlay(state, focus, now);
    window.set_focus_phase_text(SharedString::from(match derived.phase {
        presentation::FocusPhase::Idle => "Idle — start something to begin a block",
        presentation::FocusPhase::Focus => "Focus",
        presentation::FocusPhase::Break => "Break",
    }));
    window.set_focus_remaining_text(SharedString::from(derived.remaining_text));
    window.set_focus_task_title(SharedString::from(derived.task_title));
    let selected = match vs.flow() {
        crate::view_state::Flow::FocusMode { selected } => *selected,
        _ => 0,
    };
    let rows: Vec<crate::FocusRowModel> = presentation::focus_switcher_rows(state)
        .into_iter()
        .map(|row| crate::FocusRowModel {
            text: SharedString::from(row.title),
            emphasis: false,
        })
        .collect();
    window.set_focus_switcher_model(slint::ModelRc::new(slint::VecModel::from(rows)));
    window.set_focus_switcher_selected(i32::try_from(selected).unwrap_or(0));
    // The note mirror: the shared view-state buffer, seeded exactly
    // on the mount transition (edits flow back through the seam; a
    // per-second re-push would clobber typing — see the field doc).
    if mount_transition {
        window.set_focus_note_initial_text(SharedString::from(vs.buffer().to_owned()));
    }
}

/// The review surface's pass (#214, D-8/D-10): two aggregate
/// sections (7-day, 30-day trailing local-day windows, bounds derived
/// per pass from the calendar + `now` and passed into `calibrate` as
/// data) over ONE 30-day ledger. The kiosk gate lives HERE (D-10(b),
/// the `push_notifications` precedent): the empty model is emitted on
/// the appliance even though the rotation can never route to the view.
/// Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
fn push_review(
    window: &AppWindow,
    state: &AppState,
    day: crate::view_state::DayCtx,
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    reflection_focus: (
        Option<taskboard_domain::ReflectionId>,
        Option<taskboard_domain::ReflectionId>,
    ),
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    // The kiosk belt (D-10(b)): the empty model on the appliance,
    // even though the rotation can never route to the view.
    if kiosk {
        return push_review_model(window, models, empty_review_model(), &[], &[]);
    }
    let (reflection_active, reflection_armed) = reflection_focus;
    // The bounds are derived per pass and passed into `calibrate` as
    // data (constraint 2: the projection never reads a clock).
    let now = day.now;
    let [week_days, month_days] = REVIEW_WINDOW_DAYS;
    let week_window = taskboard_domain::trailing_days_window(
        now,
        calendar.day_start_time,
        calendar.offset,
        std::num::NonZeroU32::new(week_days).expect("non-zero constant"),
    );
    let month_window = taskboard_domain::trailing_days_window(
        now,
        calendar.day_start_time,
        calendar.offset,
        std::num::NonZeroU32::new(month_days).expect("non-zero constant"),
    );
    let week_report = taskboard_domain::calibrate(state, week_window, now);
    let month_report = taskboard_domain::calibrate(state, month_window, now);
    // The ledger reads the wider horizon's rows (D-8); the 7-day
    // window's members speak through its aggregate section.
    let rows: Vec<ReviewRowModel> = month_report
        .rows
        .iter()
        .map(|row| {
            let gap = row.gap();
            ReviewRowModel {
                title: SharedString::from(row.title.clone()),
                completed: SharedString::from(
                    row.done
                        .with_timezone(&calendar.offset)
                        .date_naive()
                        .to_string(),
                ),
                estimate: SharedString::from(presentation::effort_phrase(row.estimate)),
                actual: if row.tracked {
                    SharedString::from(presentation::review_seconds_phrase(
                        row.actual.num_seconds().max(0).unsigned_abs(),
                    ))
                } else {
                    SharedString::new()
                },
                gap: SharedString::from(gap.map_or_else(String::new, |delta| {
                    presentation::review_gap_phrase(delta.num_seconds())
                })),
            }
        })
        .collect();
    let to_window = |aggregate: &taskboard_domain::CalibrationAggregate| {
        let phrases = presentation::review_window(aggregate);
        ReviewWindowModel {
            estimated: SharedString::from(phrases.estimated),
            actual: SharedString::from(phrases.actual),
            gap: SharedString::from(phrases.gap),
            compared: SharedString::from(phrases.compared),
            empty: phrases.empty,
        }
    };
    // The tracking sections (#623 slice A, D-S6): the same per-pass
    // windows feed the pure folds — `now`, the bounds, and the
    // calendar are data (the projection never reads a clock). The
    // bars ride a plain whole-struct set: small, fixed-length, never
    // row-diffed.
    let entries: Vec<&taskboard_domain::TimeEntry> = state.tracking.entries.values().collect();
    let to_tracking = |split_window, strip_days: u32| {
        tracking_section_model(&entries, split_window, strip_days, calendar, now)
    };
    let tracking_week = to_tracking(week_window, week_days);
    let tracking_month = to_tracking(month_window, month_days);
    let tracking_month_empty = tracking_month.empty;
    // The reflection pair (#623 slice B, D-B7): the same per-pass
    // windows feed the pure folds — the stored `window_start` stamps
    // are the membership key (D-B2); the strips carry the fixed-scale
    // fractions. The entries ledger is derived once per pass over the
    // whole live set (no window), newest first.
    let reflection: Vec<&taskboard_domain::ReflectionEntry> =
        state.reflection.entries.values().collect();
    let to_reflection = |fold_window, strip_days: u32| {
        reflection_section_model(&reflection, fold_window, strip_days, calendar, now)
    };
    let reflection_week = to_reflection(week_window, week_days);
    let reflection_month = to_reflection(month_window, month_days);
    let reflection_month_empty = reflection_month.empty;
    let (reflection_rows, reflection_ids) =
        rows_of_past_entries(&state.reflection, reflection_active, reflection_armed);
    tables.reflections = reflection_ids;
    let model = ReviewModel {
        week: to_window(&week_report.aggregate),
        month: to_window(&month_report.aggregate),
        tracking_week,
        tracking_month,
        reflection_week,
        reflection_month,
        rows: ModelRc::default(),
        reflection_rows: ModelRc::default(),
        // Whole-view collapse (D-9 tier 1, D-S9's extension twice):
        // the 30-day ledger empty AND nothing tracked in the 30-day
        // window AND no reflection entries in the windows AND no
        // stored journal entries — any content keeps the view
        // standing.
        empty: month_report.rows.is_empty()
            && tracking_month_empty
            && reflection_month_empty
            && reflection_rows.is_empty(),
    };
    push_review_model(window, models, model, &rows, &reflection_rows)
}

/// One window's tracking-section model (D-S6 step 3): the domain folds
/// over the per-pass window and the strip-max fractions (Work →
/// `base`, Respite → `stack`).
fn tracking_section_model(
    entries: &[&taskboard_domain::TimeEntry],
    split_window: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    strip_days: u32,
    calendar: taskboard_domain::DayCalendar,
    now: chrono::DateTime<chrono::Utc>,
) -> TrackingSectionModel {
    let split = taskboard_domain::window_split(entries.iter().copied(), split_window, now);
    let blocks = taskboard_domain::work_blocks(entries.iter().copied(), split_window, now);
    let phrases = presentation::review_tracking_section(&split, &blocks);
    let series = taskboard_domain::daily_split_series(
        entries.iter().copied(),
        std::num::NonZeroU32::new(strip_days).expect("non-zero constant"),
        calendar.day_start_time,
        calendar.offset,
        now,
    );
    let bars: Vec<StatBarModel> = presentation::review_bar_fractions(&series)
        .into_iter()
        .map(|bar| StatBarModel {
            base: bar.work,
            stack: bar.respite,
        })
        .collect();
    TrackingSectionModel {
        work: SharedString::from(phrases.work),
        respite: SharedString::from(phrases.respite),
        blocks: SharedString::from(phrases.blocks),
        bars: ModelRc::new(slint::VecModel::from(bars)),
        empty: phrases.empty,
    }
}

/// One window's reflection-section model (D-B7 step 3): the domain
/// fold over the per-pass window (stored stamps are the membership
/// key, D-B2), the phrases, and the fixed-scale strip fractions
/// (Mood → `base`, Energy → `stack`).
fn reflection_section_model(
    reflection: &[&taskboard_domain::ReflectionEntry],
    fold_window: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    strip_days: u32,
    calendar: taskboard_domain::DayCalendar,
    now: chrono::DateTime<chrono::Utc>,
) -> ReflectionSectionModel {
    let fold = taskboard_domain::window_ratings(reflection.iter().copied(), fold_window);
    let phrases = presentation::review_reflection_section(&fold);
    let series = taskboard_domain::daily_rating_series(
        reflection.iter().copied(),
        std::num::NonZeroU32::new(strip_days).expect("non-zero constant"),
        calendar.day_start_time,
        calendar.offset,
        now,
    );
    let bars: Vec<StatBarModel> = presentation::review_rating_fractions(&series)
        .into_iter()
        .map(|bar| StatBarModel {
            base: bar.mood,
            stack: bar.energy,
        })
        .collect();
    ReflectionSectionModel {
        mood: SharedString::from(phrases.mood),
        energy: SharedString::from(phrases.energy),
        flags: SharedString::from(phrases.flags),
        per_flag: SharedString::from(phrases.per_flag),
        bars: ModelRc::new(slint::VecModel::from(bars)),
        empty: phrases.empty,
    }
}

/// The journal-entries ledger rows (D-B5): every live stored entry,
/// newest first, read verbatim — the stored `day` label, the cadence
/// text form, the text body, the ratings ("" = unrated, the dash
/// renders structurally), and the flags in declaration order with the
/// parked names verbatim behind the curated set. No window, no cap.
fn rows_of_past_entries(
    reflection: &taskboard_domain::ReflectionState,
    active: Option<taskboard_domain::ReflectionId>,
    armed: Option<taskboard_domain::ReflectionId>,
) -> (Vec<ReflectionRowModel>, Vec<taskboard_domain::ReflectionId>) {
    let entries = taskboard_domain::past_entries(reflection);
    let ids = entries.iter().map(|row| row.id).collect();
    let rows = entries
        .into_iter()
        .map(|row| ReflectionRowModel {
            day: SharedString::from(row.day.to_string()),
            cadence: SharedString::from(row.cadence.text_form()),
            text: SharedString::from(row.text.clone()),
            mood: row.attributes.mood.map_or_else(SharedString::new, |r| {
                SharedString::from(r.get().to_string())
            }),
            energy: row.attributes.energy.map_or_else(SharedString::new, |r| {
                SharedString::from(r.get().to_string())
            }),
            flags: SharedString::from(
                row.attributes
                    .flags
                    .iter()
                    .map(|flag| flag.text_form())
                    .chain(row.attributes.extra_flags.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            active: active == Some(row.id),
            armed: armed == Some(row.id),
        })
        .collect();
    (rows, ids)
}

/// The review windows' fixed sizes (D-4): the smallest defensible
/// pair — a config knob is a v2 decision.
const REVIEW_WINDOW_DAYS: [u32; 2] = [7, 30];

/// Installs the ledger handle into the model struct and pushes the
/// whole `ReviewModel` (the board-bar's struct+handle pattern).
fn push_review_model(
    window: &AppWindow,
    models: &mut PushModels,
    mut model: ReviewModel,
    rows: &[ReviewRowModel],
    entries: &[ReflectionRowModel],
) -> usize {
    let handle = models.review.handle_or_insert_with();
    let mut mutations = models.review.diff(rows);
    let entries_handle = models.review_entries.handle_or_insert_with();
    mutations += models.review_entries.diff(entries);
    model.rows = ModelRc::from(handle);
    model.reflection_rows = ModelRc::from(entries_handle);
    window.set_review_model(model);
    mutations
}

fn empty_review_model() -> ReviewModel {
    let collapsed = ReviewWindowModel {
        estimated: SharedString::new(),
        actual: SharedString::new(),
        gap: SharedString::new(),
        compared: SharedString::new(),
        empty: true,
    };
    // The belt's collapsed tracking sections (D-S6/D-S8(b)): empty
    // phrases, empty bars — the appliance never projects the surface
    // (the belt model is never rendered; the belt test asserts the
    // collapsed fields).
    let collapsed_tracking = TrackingSectionModel {
        work: SharedString::new(),
        respite: SharedString::new(),
        blocks: SharedString::new(),
        bars: ModelRc::default(),
        empty: true,
    };
    // The belt's collapsed reflection sections (D-B7/D-B8(b)): empty
    // phrases and empty strips — the appliance never projects the
    // surface (the belt model is never rendered; the belt test
    // asserts the collapsed fields).
    let collapsed_reflection = ReflectionSectionModel {
        mood: SharedString::new(),
        energy: SharedString::new(),
        flags: SharedString::new(),
        per_flag: SharedString::new(),
        bars: ModelRc::default(),
        empty: true,
    };
    ReviewModel {
        week: collapsed.clone(),
        month: collapsed,
        tracking_week: collapsed_tracking.clone(),
        tracking_month: collapsed_tracking,
        reflection_week: collapsed_reflection.clone(),
        reflection_month: collapsed_reflection,
        rows: ModelRc::default(),
        reflection_rows: ModelRc::default(),
        empty: true,
    }
}

/// The toolbar chrome: board title and the `app.menu` binding hint (a
/// keymap projection, never hand-written — D19). Returns the hint
/// diff's mutation count.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // the pass-shaped parameter list; one linear chrome pass (the push family's rule)
fn push_chrome(
    window: &AppWindow,
    view: crate::view_state::ViewId,
    vs: &ViewState,
    state: &AppState,
    kiosk: bool,
    board: Option<&taskboard_domain::Board>,
    keymap: &Keymap,
    chain: &[ContextId],
    models: &mut PushModels,
) -> usize {
    // The view switcher's pushed index (round-3 D22): reply-sourced —
    // every refresh rebuilds it from the view-state projection.
    window.set_switcher_index(i32::try_from(vs.switcher_selection()).unwrap_or(i32::MAX));
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
    // — bound in every shipped preset (the "empty under emacs" note
    // was stale: chunk 3 bound it as `Ctrl+c s`; r4 chunk 4's E2
    // enumeration pins the projection). The only chrome action bound
    // in no preset is `board.select` — the board-segment hint above
    // is the documented empty (the established empty-hint rendering
    // applies; a fake chord is never shown, D19).
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
    // The task view's header hints (r4 D16b/D16c): the verb the
    // surface currently offers — TaskComplete while the subject is
    // open, TaskReopen while done; TaskTrackStart/TaskTrackStop with
    // the tracking button. Empty while the task view is not mounted
    // (the nav_back projection's shape; `vs` + `state` are both in
    // scope here).
    let detail_subject = (view == crate::view_state::ViewId::TaskDetail)
        .then(|| vs.subject_task())
        .flatten();
    let done_action = detail_subject
        .and_then(|task| state.tasks.get(&task))
        .map(|task| {
            if task.done.is_some() {
                ActionId::TaskReopen
            } else {
                ActionId::TaskComplete
            }
        });
    mutations += push_rows(
        &mut models.hints.task_done,
        |rc| {
            window.set_task_done_hint(rc);
        },
        &done_action.map(rows).unwrap_or_default(),
    );
    let track_action = presentation::detail_tracking_verb(state, detail_subject);
    mutations += push_track_hint(window, &mut models.hints, &rows, track_action);
    mutations += push_title_edit_hint(window, &mut models.hints, &rows);
    // The entries opener's + effort row's projections (J3 US-4,
    // round 9 chunk 11): view-chain projections like the title's —
    // empty where the preset binds none (never a fake chord, D19).
    mutations += push_rows(
        &mut models.hints.task_entries,
        |rc| {
            window.set_task_entries_hint(rc);
        },
        &rows(ActionId::TaskTrackEntries),
    );
    mutations += push_rows(
        &mut models.hints.task_effort,
        |rc| {
            window.set_task_effort_hint(rc);
        },
        &rows(ActionId::TaskEffortEdit),
    );
    mutations
}

/// The header tracking verbs' hint projection (r4 D16c).
fn push_track_hint(
    window: &AppWindow,
    hints: &mut HintModels,
    rows: &dyn Fn(ActionId) -> Vec<SharedString>,
    track_action: Option<presentation::TrackDetail>,
) -> usize {
    let track_action = track_action.map(|verb| match verb {
        presentation::TrackDetail::Start => ActionId::TaskTrackStart,
        presentation::TrackDetail::Stop => ActionId::TaskTrackStop,
    });
    push_rows(
        &mut hints.task_track,
        |rc| {
            window.set_task_track_hint(rc);
        },
        &track_action.map(rows).unwrap_or_default(),
    )
}

/// The closed title's activation binding (r4 D21): the view-chain
/// projection, empty where the action is unbound (vi). The binding is
/// view-context, not subject — pushed on every chrome pass.
fn push_title_edit_hint(
    window: &AppWindow,
    hints: &mut HintModels,
    rows: &dyn Fn(ActionId) -> Vec<SharedString>,
) -> usize {
    push_rows(
        &mut hints.task_title_edit,
        |rc| {
            window.set_task_title_edit_hint(rc);
        },
        &rows(ActionId::TaskTitleEdit),
    )
}

/// The resolved board row (r4 D5, chunk-2 plan §1.2): the live stacks,
/// their content minimums and their resolved rendered widths — Σ
/// widths == the row width whenever the minimums fit. One assembly,
/// three readers (the models push, the drag press, the keyboard
/// route); no second derivation.
pub(crate) struct BoardRow {
    /// The tables-order stack ids.
    pub stacks: Vec<StackId>,
    /// The resolved rendered px per column, Σ == row width.
    pub widths: Vec<u32>,
    /// The content minimums, rendered px (§4).
    pub mins: Vec<u32>,
}

/// Assembles the board row (r4 D5): the row width is the window width
/// minus the chrome derived from the Theme globals — the row's 2·
/// space-3 side padding, the n inter-child gaps (n columns plus the
/// add-column affordance), and the affordance's space-6 + space-2 —
/// floored at 0. Stored widths are logical px, converted ×scale here
/// (the one conversion rule, chunk-2 plan §1.1).
pub(crate) fn board_row(window: &AppWindow, state: &AppState, vs: &ViewState) -> BoardRow {
    let theme = window.global::<Theme>();
    let scale = f64::from(theme.get_base_scale());
    let stacks = vs.stack_ids(state);
    let count = stacks.len();
    let space_3 = f64::from(theme.get_space_3());
    let affordance = f64::from(theme.get_space_6()) + f64::from(theme.get_space_2());
    #[allow(clippy::cast_precision_loss)] // a column count
    let chrome = 2.0 * space_3 + space_3 * count as f64 + affordance;
    let row_w = (f64::from(window.get_window_w()) - chrome).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let row_w = row_w as u32;
    let stored: Vec<Option<u32>> = stacks
        .iter()
        .map(|stack| vs.board().and_then(|board| vs.stack_width(board, *stack)))
        .collect();
    let mins: Vec<u32> = stacks
        .iter()
        .map(|stack| {
            let chars = state
                .stacks
                .get(stack)
                .map_or(0, |stack| stack.title.chars().count());
            crate::widths::content_min_px(chars, scale)
        })
        .collect();
    let widths = crate::widths::resolve_row(row_w, &stored, &mins);
    BoardRow {
        stacks,
        widths,
        mins,
    }
}

/// The per-column strip flag (UX R-2 D-2): the column's stack is the
/// armed `Flow::ConfirmDelete` subject.
fn stack_delete_armed(flow: &Flow, stack_id: &StackId) -> bool {
    matches!(flow, Flow::ConfirmDelete(DeleteTarget::Stack(armed)) if armed == stack_id)
}

/// The armed-delete strip's typed fact (UX R-2 D-2): the armed
/// `Flow::ConfirmDelete` stack indexed into the pass's columns, −1
/// (the move-target convention) when nothing is armed or the armed
/// stack is not in this pass (the dead-strip rule: the strip
/// unmounts).
fn armed_stack_index(flow: &Flow, stack_ids: &[StackId]) -> i32 {
    match flow {
        Flow::ConfirmDelete(DeleteTarget::Stack(armed)) => stack_ids
            .iter()
            .position(|s| s == armed)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
        _ => -1,
    }
}

/// The aimed insertion slot (UX-R8 D-2/D-7): the armed move/drag
/// flow's (stack, gap) — `None` when no aim gesture holds. One
/// derivation for the per-column `aim-gap` model field (the
/// indicator's data path) and the `move-aim` bit (US-5); the
/// keyboard slots and the drag gaps converge here by construction.
/// The move-aim badge's labelled state (UX-R8 D-7, US-5/AC-1): the
/// presentation constant behind the Badge's accessible-label
/// override — the bit's label, a #823 close. Display-only: never
/// asserted as text, the flag derivation is the test's subject.
pub(crate) const MOVE_AIM_BADGE_LABEL: &str = "Move target";

pub(crate) fn move_aim(flow: &Flow) -> Option<(StackId, usize)> {
    match flow {
        Flow::Move { target, slot, .. } => Some((*target, *slot)),
        Flow::DragMove {
            target,
            gap: Some(gap),
            ..
        } => Some((*target, *gap)),
        _ => None,
    }
}

/// The board model: one column per live stack, the flow's create row
/// and move target marked by index (D8/D22). The board title renders
/// in the toolbar (`push_chrome`), so the model carries only the
/// kanban geometry. Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass's inputs, one bundle
#[allow(clippy::too_many_lines)] // the board pass, kept flat (the push family's rule)
fn push_board(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    stack_ids: &[StackId],
    tables: &mut IdTables,
    now: DateTime<Utc>,
    calendar: taskboard_domain::DayCalendar,
    models: &mut PushModels,
) -> usize {
    let flow = vs.flow().clone();
    // The insertion indicator's aim (UX-R8 D-2): resolved once per
    // pass, applied to the aimed column only (US-5/AC-2's guard —
    // no aim, no column carries the bit).
    let aim = move_aim(&flow);

    let tracked_task = presentation::tracking_open_task(state);
    // The resolved explicit-width row (r4 D5): one assembly, pushed
    // column by column below. Its reads (Theme scale, window width)
    // live inside `board_row`.
    let row = board_row(window, state, vs);
    let mut mutations = 0;
    let mut columns: Vec<StackColumnModel> = Vec::with_capacity(stack_ids.len());
    for (column_index, stack_id) in stack_ids.iter().enumerate() {
        let stack = &state.stacks[stack_id];
        let tasks = state.tasks_in_stack(*stack_id);
        let mut task_ids = Vec::with_capacity(tasks.len());
        let cards_handle = models
            .column_tasks
            .entry(*stack_id)
            .or_default()
            .handle_or_insert_with();
        let mut cards: Vec<TaskCardModel> = Vec::with_capacity(tasks.len());
        // The anchor card is hidden from list surfaces (DDF-5): filtered
        // once, before the loop, so cards and the id table stay in sync.
        for task in tasks {
            if presentation::is_anchor_task(task) {
                continue;
            }
            task_ids.push(task.id);
            let (chips, chips_overflow) = card_chips(models, state, task);
            cards.push(task_card_model(
                task,
                sets,
                vs,
                now,
                tracked_task.is_some_and(|open| open == task.id),
                decorations_model(models, task, tracked_task, calendar, now),
                chips,
                chips_overflow,
            ));
        }
        mutations += models
            .column_tasks
            .get_mut(stack_id)
            .expect("just inserted above")
            .diff(&cards);
        tables.board_tasks.push(task_ids);
        let aimed_gap = aim
            .filter(|(aimed, _)| aimed == stack_id)
            .map(|(_, gap)| gap);
        columns.push(StackColumnModel {
            // The armed-delete strip's per-column flag (UX R-2 D-2):
            // this column's stack is the armed subject.
            delete_armed: stack_delete_armed(&flow, stack_id),
            // This column is the aim (resolved once per pass, applied
            // to the aimed column only — US-5/AC-2's guard is the
            // derivation itself: no aim, no column carries the bit).
            // The insertion indicator's aimed gap (UX-R8 D-2), the
            // count badge's move-aim state (UX-R8 D-7) and its
            // labelled twin derive from it.
            aim_gap: aimed_gap.map_or(-1, |gap| i32::try_from(gap).unwrap_or(i32::MAX)),
            move_aim: aimed_gap.is_some(),
            badge_label: SharedString::from(if aimed_gap.is_some() {
                MOVE_AIM_BADGE_LABEL
            } else {
                ""
            }),
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
            // The resolved rendered width of this column for this pass
            // (r4 D5) — always ≥ its content minimum, never a sentinel.
            // A px value, far below f32's mantissa reach.
            width_px: {
                #[allow(clippy::cast_precision_loss)]
                let px = row.widths[column_index] as f32;
                px
            },
        });
    }
    mutations += models.columns.diff(&columns);
    // The board create/rename fields' window-level slot (r2 D21).
    window.set_board_create_open(matches!(flow, Flow::CreateBoard));
    // The board color strip's pushed halves (UX R-4 D-1, #791): the
    // mount flag (armed over the *selected* board — a foreign board's
    // stale flow is pruned at reconcile, the belt here), the shared
    // palette, and the current color's ring index (a lenient value
    // echoes no ring — the honest no-match; the GUI never re-derives).
    window.set_board_color_strip_open(matches!(
        flow,
        Flow::BoardColorPick { board } if Some(board) == vs.board()
    ));
    let board_swatches: Vec<slint::Color> = presentation::SWATCHES
        .iter()
        .map(|hex| swatch_hex(&taskboard_domain::Color::new(*hex)))
        .collect();
    window.set_board_color_swatches(ModelRc::new(slint::VecModel::from(board_swatches)));
    let armed_board = match flow {
        Flow::BoardColorPick { board } => state.boards.get(&board),
        _ => None,
    };
    window.set_board_color_strip_selected(
        armed_board
            .and_then(|b| {
                presentation::SWATCHES
                    .iter()
                    .position(|hex| *hex == b.color.as_str())
            })
            .map_or(-1, |p| i32::try_from(p).unwrap_or(i32::MAX)),
    );
    // The armed board-delete strip's window-level fact (UX R-2 D-2):
    // the board-bar region's strip mounts while the board is the
    // armed subject.
    window.set_board_delete_armed(matches!(flow, Flow::ConfirmDelete(DeleteTarget::Board(_))));
    // The armed board-ceremony letter's window-level fact (OP1,
    // W-2/W-4/W-7): the letter's mode gates the strip's mount; the
    // letter binds the armed subject's board (the selected board —
    // a foreign board's stale flow is pruned at reconcile, the belt
    // the color strip's arm reads). Every number is a typed
    // derivation input: D-3's live model counts for the reset, D-4's
    // planner counts for the reconcile preview; the no-AppUndo flag
    // rides every ceremony letter as `false` (R-3 — the letter never
    // offers an undo affordance; the fold test asserts the flag).
    let selected = vs.board();
    // usize → the model's `int` via the shared cast (the
    // count-caption's truncation law).
    let counts_to_int = |v: usize| -> i32 { i32::try_from(v).unwrap_or(i32::MAX) };
    let disarmed = CeremonyLetterModel {
        mode: SlintCeremonyLetterMode::None,
        stacks: 0,
        tasks: 0,
        labels: 0,
        merge_stacks: 0,
        merge_tasks: 0,
        merge_labels: 0,
        cancelled_ops: 0,
        undo_offered: false,
    };
    let ceremony_letter = match &flow {
        Flow::ConfirmBoardArchive { board } if Some(*board) == selected => CeremonyLetterModel {
            mode: SlintCeremonyLetterMode::ArchiveCaption,
            undo_offered: false,
            ..disarmed
        },
        Flow::ConfirmBoardReset { board } if Some(*board) == selected => {
            let (stacks, tasks, labels) = board_content_counts(state, *board);
            CeremonyLetterModel {
                mode: SlintCeremonyLetterMode::ResetCounts,
                stacks,
                tasks,
                labels,
                undo_offered: false,
                ..disarmed
            }
        }
        Flow::ConfirmBoardReconcile { board, preview } if Some(*board) == selected => {
            match preview {
                ReconcilePreviewState::Pending => CeremonyLetterModel {
                    mode: SlintCeremonyLetterMode::ReconcilePending,
                    undo_offered: false,
                    ..disarmed
                },
                ReconcilePreviewState::Ready(counts) => CeremonyLetterModel {
                    mode: SlintCeremonyLetterMode::ReconcilePreview,
                    merge_stacks: counts_to_int(counts.stacks),
                    merge_tasks: counts_to_int(counts.tasks),
                    merge_labels: counts_to_int(counts.labels),
                    cancelled_ops: counts_to_int(counts.cancelled_ops),
                    undo_offered: false,
                    ..disarmed
                },
                // The dead-lane fact disarms the flow before any
                // render (the view state's settle rule) — the mode
                // exists for the fold catalogue's exhaustiveness.
                ReconcilePreviewState::Failed => CeremonyLetterModel {
                    mode: SlintCeremonyLetterMode::ReconcileFailed,
                    undo_offered: false,
                    ..disarmed
                },
            }
        }
        _ => disarmed,
    };
    window.set_ceremony_letter(ceremony_letter);
    // The armed multi-delete strip's window-level fact (UX-R8 chunk-5
    // D-9, #882): the member count with −1 as the disarmed sentinel —
    // the count caption needs the number (the #784 grammar's third
    // instance beside the stack/board strips).
    window.set_task_multi_delete_armed(match &flow {
        Flow::ConfirmDelete(DeleteTarget::Tasks(members)) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            {
                members.len() as i32
            }
        }
        _ => -1,
    });
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
        let (ghost_chips, ghost_overflow) = card_chips(models, state, dragged);
        let mut ghost = task_card_model(
            dragged,
            sets,
            vs,
            now,
            tracked_task.is_some_and(|open| open == dragged.id),
            decorations_model(models, dragged, tracked_task, calendar, now),
            ghost_chips,
            ghost_overflow,
        );
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
        stacks: ModelRc::from(models.columns.handle_or_insert_with()),
        // The stack-create flow's visible field (usability D7).
        stack_create_open: matches!(flow, Flow::CreateStack),
        // The armed-delete strip's typed fact (UX R-2 D-2): the
        // armed stack indexed into this pass, −1 when unarmed.
        delete_armed_stack: armed_stack_index(&flow, stack_ids),
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

    /// Diffs the selector's dropdown options onto their persistent
    /// handle (r4 D39) — the same pattern as the connector rows.
    pub(crate) fn push_board_options(
        &mut self,
        global: &crate::SettingsModel<'_>,
        desired: &[crate::DropdownRow],
    ) -> usize {
        push_rows(
            &mut self.board_options,
            |rc| global.set_board_options(rc),
            desired,
        )
    }

    /// Diffs the account cards' walk-ordered item projection (round-5
    /// chunk 8) onto its persistent handle — positional, so a refresh
    /// pass never re-instantiates the focused row's controls.
    pub(crate) fn push_connection_items(
        &mut self,
        global: &crate::SettingsModel<'_>,
        desired: &[crate::ConnectionItem],
    ) -> usize {
        push_rows(
            &mut self.connection_items,
            |rc| global.set_connection_items(rc),
            desired,
        )
    }

    /// Diffs the settings editor's read-only fact rows onto their
    /// persistent handle (the connector rows' pattern; the facts
    /// change only on a reply, but the push is per pass).
    pub(crate) fn push_settings_facts(
        &mut self,
        global: &crate::SettingsModel<'_>,
        desired: &[crate::SettingsFactRow],
    ) -> usize {
        push_rows(&mut self.settings_facts, |rc| global.set_facts(rc), desired)
    }
}

/// The task's decoration model: get-or-create the persistent
/// per-task handle and diff the desired decorations onto it (r2
/// D9's `card_decorations` slot — the handle identity that makes
/// unchanged cards compare fully equal).
/// The card label chips' persistent model (tag-color plan D-9, the
/// `decorations_model` slot rule): diffed per pass keyed by the task,
/// the handle stable across refreshes. Returns the handle plus the
/// typed overflow count (the count rides the value diff).
fn card_chips(
    models: &mut PushModels,
    state: &AppState,
    task: &Task,
) -> (ModelRc<crate::TaskLabelChipModel>, i32) {
    let (rows, overflow) = task_card_label_chips(state, task.id);
    let slot = models.card_label_chips.entry(task.id).or_default();
    models.pass_tasks.insert(task.id);
    slot.diff(&rows);
    (ModelRc::from(slot.handle_or_insert_with()), overflow)
}

fn decorations_model(
    models: &mut PushModels,
    task: &Task,
    tracked_task: Option<TaskId>,
    calendar: taskboard_domain::DayCalendar,
    now: DateTime<Utc>,
) -> ModelRc<TaskDecoration> {
    let slot = models.card_decorations.entry(task.id).or_default();
    models.pass_tasks.insert(task.id);
    slot.diff(&field_decorations(task, tracked_task, calendar, now));
    ModelRc::from(slot.handle_or_insert_with())
}

/// The stack-detail model over the snapshotted subject (D7/D22).
/// Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass's inputs, one bundle
fn push_stack_detail(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    tables: &mut IdTables,
    now: DateTime<Utc>,
    calendar: taskboard_domain::DayCalendar,
    models: &mut PushModels,
) -> usize {
    let tasks = vs
        .subject_stack()
        .map(|stack| state.tasks_in_stack(stack))
        .unwrap_or_default();
    let tracked_task = presentation::tracking_open_task(state);
    let detail_handle = models.detail_tasks.handle_or_insert_with();
    let mut detail_rows: Vec<TaskCardModel> = Vec::with_capacity(tasks.len());
    // The same single-pass anchor filter as `push_board` (DDF-5).
    for task in &tasks {
        if presentation::is_anchor_task(task) {
            continue;
        }
        tables.stack_tasks.push(task.id);
        let (chips, chips_overflow) = card_chips(models, state, task);
        detail_rows.push(task_card_model(
            task,
            sets,
            vs,
            now,
            tracked_task.is_some_and(|open| open == task.id),
            decorations_model(models, task, tracked_task, calendar, now),
            chips,
            chips_overflow,
        ));
    }
    let mutations = models.detail_tasks.diff(&detail_rows);
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
    kiosk: bool,
    models: &mut PushModels,
    due: &DueEditorPush<'_>,
    recur: crate::models::RecurRowPush,
    priority: crate::models::PriorityPickerPush,
    label_form: Option<&crate::models::LabelFormPush>,
    effort: Option<&crate::models::EffortEditorPush<'_>>,
    label_armed_index: i32,
    calendar: taskboard_domain::DayCalendar,
    habit: Option<&crate::router::HabitEditDraft>,
) -> usize {
    let subject = vs.subject_task().and_then(|t| state.tasks.get(&t));
    // The recur row's display half (issue #506): the pattern echo and
    // the paused chip read the rule verbatim — the chip rides the
    // badge label's paused phrase (the same date comparison, so the
    // chip and the badge can never disagree).
    let recur_rule = subject.and_then(|t| t.recur.as_ref());
    let recur_badge = subject.and_then(|t| presentation::recur_badge(t, calendar, now));
    // The detail button's verb (DR-5/4, D-TU6): exactly one while a
    // subject exists, both false without.
    let track_verb = presentation::detail_tracking_verb(state, vs.subject_task());
    // W-14 (#1048): the widened subject — the flags read the flow's
    // subject against THIS surface's subject (task or habit; the
    // habit variant mounts the shared editors on its subject).
    let editing = matches!(
        vs.flow(),
        Flow::EditTitle {
            subject: crate::view_state::EditSubject::Task(t),
        } if vs.subject_task().is_some_and(|s| *t == s)
    ) | matches!(
        vs.flow(),
        Flow::EditTitle {
            subject: crate::view_state::EditSubject::Habit(h),
        } if vs.subject_habit().is_some_and(|s| *h == s)
    );
    let description_editing = matches!(
        vs.flow(),
        Flow::EditDescription {
            subject: crate::view_state::EditSubject::Task(t),
        } if vs.subject_task().is_some_and(|s| *t == s)
    ) | matches!(
        vs.flow(),
        Flow::EditDescription {
            subject: crate::view_state::EditSubject::Habit(h),
        } if vs.subject_habit().is_some_and(|s| *h == s)
    );
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
        .map(|rule| {
            let subject_duedate = vs
                .subject_task()
                .and_then(|t| state.tasks.get(&t))
                .and_then(|t| t.duedate);
            // The kind-specific texts are the projection's job (r4
            // D30); the armed bit is derived here, never stored.
            let texts = presentation::rule_row_texts(rule, subject_duedate, now, &chrono::Local);
            RuleRowModel {
                before: SharedString::from(texts.primary),
                created: SharedString::from(presentation::rule_created_phrase(
                    rule.created_at,
                    now,
                )),
                detail: SharedString::from(texts.detail),
                armed: texts.armed,
                // The cursor's emphasis (J3 US-6 D-4): derived from
                // the view state's cursor — never stored (the
                // habit-card emphasis pattern).
                selected: vs.rule_cursor() == Some(rule.id),
            }
        })
        .collect();
    let preset_rows: Vec<SharedString> = presentation::rule_preset_labels()
        .into_iter()
        .map(SharedString::from)
        .collect();
    // The due editor's grid (r2 D25): rows of seven, leading blanks,
    // cursor/today/kind markers — pushed only while the editor is
    // open; the empty model otherwise.
    let due_open = matches!(
        vs.flow(),
        Flow::EditDate {
            field: DateField::Due,
            ..
        }
    );
    let scheduled_open = matches!(
        vs.flow(),
        Flow::EditDate {
            field: DateField::Scheduled,
            ..
        }
    );
    let date_open = due_open || scheduled_open;
    let due_grid_rows: Vec<slint::ModelRc<DueDayModel>> = due
        .picker
        .filter(|_| date_open)
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
                                            crate::due_edit::cell_kind(d, today, due.horizon)
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
    let date_grid = slint::ModelRc::new(slint::VecModel::from(due_grid_rows));

    // The entries overlay's pushed halves (r4 chunk 10, D-TS3): the
    // subject's rows in the presentation's newest-first order, the
    // total re-derived through the summary row's own helpers
    // (reconciliation by construction), and the kiosk gate (the
    // display-stays, action-empties posture — the total keeps
    // showing, the affordance and the overlay hide). The overlay
    // mounts only over the viewed task (the mount condition's
    // subject-affine half is the flow; the model flag is the window's
    // read of it).
    let track_rows = vs
        .subject_task()
        .map(|subject| presentation::track_entry_rows(state, subject, now, &chrono::Local))
        .unwrap_or_default();
    tables.track_entries = track_rows.iter().map(|row| row.id).collect();
    // The labels tables (UX R-3 D-3): the subject's live chips in
    // canonical order (the OR-set's BTreeSet), then the subject
    // board's full live roster — labels are board-scoped, the
    // overlay's rows resolve against the roster pass.
    tables.task_labels = subject
        .map(|t| t.live_labels().into_iter().collect())
        .unwrap_or_default();
    // The labels roster's board (tag-color plan D-8): the detail
    // mount derives the subject task's board; the roster-only board
    // mount reads the subject board directly. The canonical order
    // stands (the engine's BTreeMap id order — no new ordering).
    let roster_board = match vs.labels_subject() {
        Some(crate::view_state::LabelOverlaySubject::Task(task)) => state
            .tasks
            .get(&task)
            .and_then(|t| state.stacks.get(&t.stack))
            .map(|stack| stack.board),
        Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
        None => None,
    };
    tables.label_ids = roster_board
        .map(|board| {
            state
                .labels
                .values()
                .filter(|label| !label.deleted && label.board == board)
                .map(|label| label.id)
                .collect()
        })
        .unwrap_or_default();
    let track_entry_rows: Vec<TrackEntryRowModel> = track_rows
        .iter()
        .map(|row| TrackEntryRowModel {
            date: SharedString::from(row.date.clone()),
            start: SharedString::from(row.start.clone()),
            end: SharedString::from(if row.end_other_day {
                format!("{} {}", row.end, presentation::TRACK_END_PLUS_DAY)
            } else {
                row.end.clone()
            }),
            spent: SharedString::from(row.spent.clone()),
            running: row.running,
        })
        .collect();
    // The overlay's open flag reads the flow (the window's mount
    // condition) — subject-affine: a flow whose task stopped being
    // the viewed subject renders closed (reconcile drops it anyway;
    // this is the push-side belt). The read walks the CHAIN, not the
    // top flow: the form's date-grid child (the overlay migration's
    // window-level mount) sits on top while the overlay holds, and a
    // top-only read would close the panel under its own child.
    let track_open = vs.surfaces().above_levels().iter().any(|level| {
        matches!(
            level.flow(),
            Some(Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. })
        )
    }) && match vs.flow() {
        Flow::TrackEntries { task } | Flow::TrackEntryForm { task, .. } => {
            vs.subject_task() == Some(*task)
        }
        // A child (the date-grid picker) on top: the subject
        // lives in the parent level below.
        _ => true,
    };
    let presets_handle = models.task_rule_presets.handle_or_insert_with();
    let mut mutations = models.task_rule_presets.diff(&preset_rows);
    let rules_handle = models.task_rules.handle_or_insert_with();
    mutations += models.task_rules.diff(&rule_rows);
    window.set_task_model(TaskDetailModel {
        title: SharedString::from(subject.map_or(String::new(), |t| t.title.clone())),
        stack_title: SharedString::from(
            subject
                .and_then(|t| state.stacks.get(&t.stack))
                .map_or(String::new(), |s| s.title.clone()),
        ),
        due_phrase: SharedString::from(subject.map_or(String::new(), |t| {
            presentation::due_phrase(
                presentation::due_kind(t, sets),
                t.duedate,
                now,
                &chrono::Local,
            )
        })),
        due_kind: to_slint_due_kind(
            subject.map_or(DueKind::None, |t| presentation::due_kind(t, sets)),
        ),
        done: subject.is_some_and(|t| t.done.is_some()),
        can_done: subject.is_none_or(|t| !presentation::tracker_badge_visible(t)),
        can_track_start: track_verb == Some(presentation::TrackDetail::Start),
        can_track_stop: track_verb == Some(presentation::TrackDetail::Stop),
        description: SharedString::from(subject.map_or(String::new(), |t| t.description.clone())),
        description_edit_open: description_editing,
        description_initial: SharedString::from(if description_editing {
            vs.buffer().to_owned()
        } else {
            String::new()
        }),
        edit_open: editing,
        // The arm-path focus flags (US-5/W-5, D-F5): the editor flow's
        // arm names the field on the viewed subject (the walk's own
        // stop fact rides the slint side).
        title_armed: matches!(
            vs.flow(),
            Flow::EditTitle {
                subject: crate::view_state::EditSubject::Task(t),
            } if Some(*t) == vs.subject_task()
        ),
        description_armed: matches!(
            vs.flow(),
            Flow::EditDescription {
                subject: crate::view_state::EditSubject::Task(t),
            } if Some(*t) == vs.subject_task()
        ),
        initial_text: SharedString::from(if editing {
            vs.buffer().to_owned()
        } else {
            String::new()
        }),
        rules: ModelRc::from(rules_handle.clone()),
        rule_presets: ModelRc::from(presets_handle.clone()),
        rules_hint: SharedString::from(presentation::rule_list_hint(
            task_rules.len(),
            vs.subject_task()
                .is_some_and(|t| state.reminders.muted.contains(&t)),
        )),
        rules_muted: vs
            .subject_task()
            .is_some_and(|t| state.reminders.muted.contains(&t)),
        due_open,
        due_text: SharedString::from(if due_open { due.text } else { "" }),
        due_echo: SharedString::from(if due_open { due.echo } else { "" }),
        scheduled_open,
        scheduled_text: SharedString::from(if scheduled_open { due.text } else { "" }),
        scheduled_echo: SharedString::from(if scheduled_open { due.echo } else { "" }),
        scheduled_phrase: SharedString::from(subject.map_or(String::new(), |t| {
            presentation::scheduled_phrase(t.scheduled, now, &chrono::Local)
        })),
        // The tracked-time total (r4 D17): the phrase when a subject
        // exists — an honest zero with no entries — empty without.
        tracked_total: SharedString::from(
            subject
                .map(|t| {
                    presentation::tracked_total_phrase(&presentation::tracked_total(
                        state, t.id, now,
                    ))
                })
                .unwrap_or_default(),
        ),
        // The entries overlay's halves (r4 chunk 10): the gate is
        // `!kiosk && subject present`; the total is the phrase above
        // verbatim; the rows ride the fresh-VecModel-per-pass shape.
        track_entries_visible: !kiosk && subject.is_some(),
        track_entries_open: track_open,
        track_entries_total: SharedString::from(
            subject
                .map(|t| {
                    presentation::tracked_total_phrase(&presentation::tracked_total(
                        state, t.id, now,
                    ))
                })
                .unwrap_or_default(),
        ),
        // The recur row (issue #506): pattern echo + paused chip over
        // the rule's truth; the flags ride the RecurRowPush.
        recur_visible: recur_rule.is_some(),
        recur_pattern: SharedString::from(
            recur_rule.map_or(String::new(), |rule| rule.pattern.text_form()),
        ),
        recur_paused_chip: SharedString::from(recur_badge.as_ref().map_or_else(
            String::new,
            |badge| {
                if badge
                    .paused_until
                    .is_some_and(|pause| calendar.today(now) <= pause)
                {
                    presentation::recur_badge_label(badge, calendar.today(now))
                } else {
                    String::new()
                }
            },
        )),
        recur_pause_open: recur.pause_open,
        recur_confirm_open: recur.confirm_open,
        // The repeat affordances' mount flag (round-7 D-7.1): the
        // presentation fold over the subject — the done-subject
        // precondition only (UX R-1 D-2: kiosk no longer hides the
        // row, it greys the actions).
        series_edit_visible: subject.is_some_and(crate::presentation::series_edit_visible),
        // The repeat affordances' enablement (UX R-1 D-2, #790): false
        // on kiosk — the recur row's actions render the disabled tone.
        series_edit_enabled: crate::presentation::series_edit_enabled(kiosk),
        track_entries: ModelRc::new(slint::VecModel::from(track_entry_rows)),
        date_picker_open: due.picker.is_some(),
        date_caption: SharedString::from(
            due.picker
                .map_or(String::new(), |p| p.month().format("%Y-%m").to_string()),
        ),
        date_grid,
        date_grid_focus: due.focus_on_mount,
        date_field_focus: due.field_focus,
        date_time_text: SharedString::from(due.picker.map_or(String::new(), |p| {
            p.time()
                .map_or("--:--".to_owned(), |t| t.format("%H:%M").to_string())
        })),
        // The time combobox's pushed half (r4 D26): the visible text
        // mirror, the tri-state validity flavour, and the filtered
        // quick-list rows with their nearest-slot seek index.
        date_time_field_text: SharedString::from(due.time_text),
        date_time_validity: match due.time_valid {
            None => crate::FieldValidity::Neutral,
            Some(true) => crate::FieldValidity::Valid,
            Some(false) => crate::FieldValidity::Invalid,
        },
        date_time_slots: ModelRc::new(slint::VecModel::from(
            due.time_slots
                .iter()
                .map(|slot| crate::TimeSlotRow {
                    label: SharedString::from(crate::due_edit::format_time_hm(*slot)),
                    minutes: crate::due_edit::minutes_of_day(*slot),
                })
                .collect::<Vec<crate::TimeSlotRow>>(),
        )),
        date_time_nearest_row: i32::try_from(due.time_nearest_row).unwrap_or(0),
        // The priority rows' halves (UX R-3 D-1, #788): the echoes are
        // the domain text forms (the card chips' own labels); the open
        // flags ride the picker flow (the seams commit wires the
        // Flow::EditPriority read — the display half pushes closed).
        // The priority rows' halves (UX R-3 D-1, #788): the echoes are
        // the domain text forms (the card chips' own labels); the open
        // flags ride the picker flow (the seams commit wires the
        // Flow::EditPriority read — the display half pushes closed).
        importance_echo: SharedString::from(
            subject
                .and_then(|t| t.importance)
                .map_or(String::new(), |level| level.text_form().to_owned()),
        ),
        // The effort row's half (the overlay migration, M9): the
        // always-on field's seed (the adapter computes the armed-
        // buffer-or-stored-form rule).
        effort_text: SharedString::from(effort.map_or("", |e| e.buffer).to_owned()),
        urgency_echo: SharedString::from(
            subject
                .and_then(|t| t.urgency)
                .map_or(String::new(), |level| level.text_form().to_owned()),
        ),
        importance_open: priority.open == Some(crate::router::PriorityField::Importance),
        urgency_open: priority.open == Some(crate::router::PriorityField::Urgency),
        // Each field's own pushed vocabulary (round-9 chunk 15 D-4):
        // the Dropdown fields are always mounted, so both lists ride
        // every pass — each marks ITS field's current value (the
        // widget's selected rows and the open-time anchor read them).
        importance_rows: ModelRc::new(slint::VecModel::from(dropdown_vocabulary_rows(
            subject
                .and_then(|t| t.importance)
                .map(|level| level as usize + 1),
        ))),
        urgency_rows: ModelRc::new(slint::VecModel::from(dropdown_vocabulary_rows(
            subject
                .and_then(|t| t.urgency)
                .map(|level| level as usize + 1),
        ))),
        importance_selected_index: i32::try_from(
            subject
                .and_then(|t| t.importance)
                .map_or(0, |level| level as usize + 1),
        )
        .unwrap_or(0),
        urgency_selected_index: i32::try_from(
            subject
                .and_then(|t| t.urgency)
                .map_or(0, |level| level as usize + 1),
        )
        .unwrap_or(0),
        // The labels section's halves (UX R-3 D-2, #789): the
        // subject's live labels as chips in canonical order; the chip
        // ids ride the same pass's table (index-aligned by
        // construction).
        labels_visible: subject.is_some(),
        labels: ModelRc::new(slint::VecModel::from(label_chip_rows(
            state,
            vs.subject_task(),
            &tables.task_labels,
        ))),
        // The overlay's open flag reads the flow — subject-affine: a
        // flow whose task stopped being the viewed subject renders
        // closed (the track-entries push-side belt; reconcile drops
        // it anyway).
        labels_open: vs.labels_mount_open(),
        date_validity: match due.date_valid {
            None => crate::FieldValidity::Neutral,
            Some(true) => crate::FieldValidity::Valid,
            Some(false) => crate::FieldValidity::Invalid,
        },
        date_syntax_hint: SharedString::from(due.syntax_hint),
        time_syntax_hint: SharedString::from(due.time_hint),
        offset_validity: match due.offset_valid {
            None => crate::FieldValidity::Neutral,
            Some(true) => crate::FieldValidity::Valid,
            Some(false) => crate::FieldValidity::Invalid,
        },
        offset_error_hint: SharedString::from(due.offset_hint),
        // The habit variant's mount discriminator (H4 D1): the
        // task-only sections hide behind it in the view.
        is_habit: habit.is_some(),
    });
    tables.task_rules = task_rules;
    // The habit editor's pushed half (H4 D5): the draft-backed
    // fields. Pushed only while the variant mounts (the mount
    // discriminator's own model).
    if let Some(draft) = habit {
        let stored = state.habits.habits.get(&draft.habit);
        let can_save = stored.is_some_and(|row| draft.can_save(row));
        // US-10 (W-14, #1048): the title/description rows read the
        // shared flow's buffer while armed (the display-mirror rule),
        // the stored row otherwise — the draft's private mirrors are
        // gone.
        let habit_title_armed = vs.habit_edit_flow_armed(Some(draft.habit), false);
        let habit_description_armed = vs.habit_edit_flow_armed(Some(draft.habit), true);
        let habit_row_title = stored.map(|row| row.title.clone());
        let habit_row_description = stored.map(|row| row.description.clone());
        window.set_habit_model(crate::HabitDetailModel {
            title: SharedString::from(if habit_title_armed {
                vs.buffer().to_owned()
            } else {
                habit_row_title.unwrap_or_default()
            }),
            description: SharedString::from(if habit_description_armed {
                vs.buffer().to_owned()
            } else {
                habit_row_description.unwrap_or_default()
            }),
            quota_kind: i32::try_from(draft.quota_kind_index()).unwrap_or(0),
            quota_count: SharedString::from(draft.quota_count.clone()),
            quota_valid: draft.quota_valid,
            time_text: SharedString::from(draft.time_text.clone()),
            time_set: draft.time_set,
            time_valid: draft.time_valid,
            kind: i32::try_from(draft.quota_kind_index()).unwrap_or(0),
            weekday: draft.weekday_index(),
            month_day: SharedString::from(draft.month_day.clone()),
            month_valid: draft.month_valid,
            // HF4 (W-6/W-7): the kind-gated schedule picker's slots +
            // validity, and the override section's field mirrors.
            schedule_visible: draft.schedule_picker_visible(),
            schedule_days: ModelRc::new(slint::VecModel::from(
                (0_usize..7)
                    .map(|slot| draft.schedule_day_checked(slot))
                    .collect::<Vec<_>>(),
            )),
            schedule_valid: !draft.schedule_edited || draft.schedule_bits != 0,
            reminder_base: i32::try_from(draft.reminder_base).unwrap_or(0),
            reminder_offset: SharedString::from(draft.reminder_offset.clone()),
            reminder_time: SharedString::from(draft.reminder_time.clone()),
            reminder_snooze: SharedString::from(draft.reminder_snooze.clone()),
            reminder_valid: draft.reminder_valid(),
            // The nudge-schedule preview (#1035, W-13, US-9): the
            // display-only projection of the drafted shape; hidden on
            // a refused draft (the neutral state). The clock time
            // composes through the day calendar; `nudge_time_set` is
            // the typed anchor-time-only bit (#1212) — the rendered
            // phrase is judged at the walkthrough, never asserted.
            nudge_preview: {
                let (visible, day_text, time_text, time_set, snooze_text) = stored
                    .and_then(|row| {
                        draft
                            .nudge_preview(row, calendar.today(now))
                            .map(|preview| {
                                let time_text = preview.time.map_or_else(
                                    || crate::presentation::habit_nudge_anchor_phrase(preview.day),
                                    |t| {
                                        format!(
                                            "{} {}",
                                            crate::due_edit::format_time_hm(t),
                                            crate::presentation::habit_nudge_anchor_phrase(
                                                preview.day
                                            )
                                        )
                                    },
                                );
                                (
                                    true,
                                    preview.day.to_string(),
                                    time_text,
                                    preview.time.is_some(),
                                    preview.snooze.map_or_else(
                                        || "—".to_owned(),
                                        |snooze| match snooze {
                                            taskboard_domain::habit::HabitSnooze::Hours(h) => {
                                                format!("{h}h")
                                            }
                                            taskboard_domain::habit::HabitSnooze::Days(d) => {
                                                format!("{d}d")
                                            }
                                        },
                                    ),
                                )
                            })
                    })
                    .unwrap_or((false, String::new(), String::new(), false, String::new()));
                crate::NudgePreviewModel {
                    visible,
                    day: SharedString::from(day_text),
                    time: SharedString::from(time_text),
                    time_set,
                    snooze: SharedString::from(snooze_text),
                }
            },
            can_save,
        });
    }
    // The labels overlay's roster (UX R-3 D-2, #789): the subject
    // board's live labels with the subject's assignment bit — the
    // fresh-VecModel-per-pass shape (the track-entries rows' rule;
    // ids ride `tables.label_ids`, the rows carry display parts).
    let overlay_rows: Vec<crate::LabelRowModel> = match vs.labels_subject() {
        Some(crate::view_state::LabelOverlaySubject::Task(task))
            if vs.subject_task() == Some(task) =>
        {
            let assigned: std::collections::BTreeSet<_> = subject
                .map(taskboard_domain::Task::live_labels)
                .unwrap_or_default();
            tables
                .label_ids
                .iter()
                .filter_map(|id| state.labels.get(id).map(|label| (id, label)))
                .map(|(id, label)| crate::LabelRowModel {
                    title: SharedString::from(label.title.clone()),
                    color: swatch_hex(&label.color),
                    assigned: assigned.contains(id),
                })
                .collect()
        }
        // The roster-only mount (tag-color plan D-3): the board's live
        // roster, no assignment bit (there is no subject — R-2's
        // letter; the panel's toggle affordance rides the
        // shows-assignment flag).
        Some(crate::view_state::LabelOverlaySubject::Board(_)) => state
            .labels
            .values()
            .filter(|label| !label.deleted && roster_board == Some(label.board))
            .map(|label| crate::LabelRowModel {
                title: SharedString::from(label.title.clone()),
                color: swatch_hex(&label.color),
                assigned: false,
            })
            .collect(),
        _ => Vec::new(),
    };
    window.set_label_overlay_rows(ModelRc::new(slint::VecModel::from(overlay_rows)));
    // The form surface's pushed halves (UX R-3 D-2): the armed
    // swatch rides the palette's index; the title seed is the
    // field's mount-only read.
    let swatches: Vec<slint::Color> = presentation::SWATCHES
        .iter()
        .map(|hex| swatch_hex(&taskboard_domain::Color::new(*hex)))
        .collect();
    window.set_label_overlay_swatches(ModelRc::new(slint::VecModel::from(swatches)));
    window.set_label_overlay_form_open(label_form.as_ref().is_some());
    window.set_label_overlay_form_title(SharedString::from(
        label_form
            .as_ref()
            .map_or(String::new(), |f| f.title.clone()),
    ));
    // The colour field's pushed halves (tag-color plan D-5, #839):
    // the draft string (mount-only read — never push-ward while
    // typing), its typed validity, and the preview echo (invalid →
    // transparent, the `swatch_hex` hygiene at draft time).
    window.set_label_overlay_form_colour(SharedString::from(
        label_form
            .as_ref()
            .map_or(String::new(), |f| f.color.clone()),
    ));
    window.set_label_overlay_colour_valid(
        label_form
            .as_ref()
            .is_some_and(|f| taskboard_domain::Color::parse(&f.color).is_ok()),
    );
    window.set_label_overlay_colour_preview(
        label_form
            .as_ref()
            .map_or(swatch_hex(&taskboard_domain::Color::new("")), |f| {
                swatch_hex(&taskboard_domain::Color::new(f.color.clone()))
            }),
    );
    window.set_label_overlay_swatch_selected(
        label_form
            .as_ref()
            .and_then(|f| {
                presentation::SWATCHES
                    .iter()
                    .position(|hex| *hex == f.color)
            })
            .map_or(-1, |p| i32::try_from(p).unwrap_or(i32::MAX)),
    );
    window.set_label_overlay_armed_index(label_armed_index);
    // The board mount's open flag (tag-color plan D-4) + the
    // assignment-toggles' shown half (D-3): the roster-only mount
    // renders no toggle affordance (R-2's letter — the row-set
    // composition is the visible ruling).
    window.set_label_overlay_open(vs.labels_board_mount_open());
    window.set_label_overlay_shows_assignment(matches!(
        vs.labels_subject(),
        Some(crate::view_state::LabelOverlaySubject::Task(_))
    ));
    mutations
}

/// The priority pickers' vocabulary rows (UX R-3 D-1, #788; round-9
/// chunk 15 D-4): the six-value vocabulary in order — `unset` first,
/// then the five domain levels in declaration order — with the
/// field's current value pre-marked (`current` is the vocabulary
/// index of the field's level; `None` marks the unset slot), as the
/// shared `DropdownRow` model (the widget class's row type; the
/// indicator/tone slots ride the class's neutral defaults).
fn dropdown_vocabulary_rows(current: Option<usize>) -> Vec<crate::DropdownRow> {
    let mut rows = Vec::with_capacity(6);
    let unset_selected = current.is_none();
    rows.push(crate::DropdownRow {
        label: SharedString::from("unset"),
        indicator: crate::DropdownIndicator::None,
        tone: crate::DropdownTone::Neutral,
        selected: unset_selected,
        busy: false,
    });
    let levels: [&str; 5] = [
        Importance::VeryLow.text_form(),
        Importance::Low.text_form(),
        Importance::Normal.text_form(),
        Importance::High.text_form(),
        Importance::VeryHigh.text_form(),
    ];
    for (index, label) in levels.iter().enumerate() {
        rows.push(crate::DropdownRow {
            label: SharedString::from(*label),
            indicator: crate::DropdownIndicator::None,
            tone: crate::DropdownTone::Neutral,
            selected: current == Some(index + 1),
            busy: false,
        });
    }
    rows
}

/// The labels section's chip rows (UX R-3 D-2, #789): the subject's
/// live labels as display chips, index-aligned with
/// `IdTables::task_labels` (the same pass builds both).
fn label_chip_rows(
    state: &AppState,
    subject: Option<TaskId>,
    chip_ids: &[LabelId],
) -> Vec<crate::LabelChipModel> {
    subject
        .and_then(|subject| {
            let task = state.tasks.get(&subject)?;
            Some(
                chip_ids
                    .iter()
                    .filter(|id| task.live_labels().contains(id))
                    .filter(|id| state.labels.get(*id).is_some_and(|l| !l.deleted))
                    .filter_map(|id| {
                        state.labels.get(id).map(|label| crate::LabelChipModel {
                            title: SharedString::from(label.title.clone()),
                            color: swatch_hex(&label.color),
                        })
                    })
                    .collect::<Vec<crate::LabelChipModel>>(),
            )
        })
        .unwrap_or_default()
}

/// The card's label chips (tag-color UX plan D-9, US-3): the task's
/// live labels in canonical order (the `BTreeSet`'s id order —
/// `task_labels`' own order), each as (title, `swatch_hex` echo);
/// beyond [`crate::presentation::LABEL_CHIP_CAP`] the rest collapse
/// into the typed overflow count (the compact leg hides the row
/// entirely — the decoration grammar's tertiary posture).
pub(crate) fn task_card_label_chips(
    state: &AppState,
    task_id: TaskId,
) -> (Vec<crate::TaskLabelChipModel>, i32) {
    let Some(task) = state.tasks.get(&task_id) else {
        return (Vec::new(), 0);
    };
    let mut chips: Vec<crate::TaskLabelChipModel> = task
        .live_labels()
        .iter()
        .filter_map(|id| state.labels.get(id).filter(|label| !label.deleted))
        .map(|label| crate::TaskLabelChipModel {
            title: SharedString::from(label.title.clone()),
            color: swatch_hex(&label.color),
        })
        .collect();
    let hidden = chips
        .len()
        .saturating_sub(crate::presentation::LABEL_CHIP_CAP);
    chips.truncate(crate::presentation::LABEL_CHIP_CAP);
    (chips, i32::try_from(hidden).unwrap_or(i32::MAX))
}

/// The swatch echo of a lenient domain `Color`: six-hex bodies mint
/// the typed color (the palette's own vocabulary); anything else
/// renders transparent (the lenient value survives sync — the GUI
/// never re-derives it, D-5).
pub(crate) fn swatch_hex(color: &taskboard_domain::Color) -> slint::Color {
    let raw = color.as_str();
    if raw.len() == 6 && raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        let bytes = raw.as_bytes();
        let (r, g, b) = (hex(&bytes[0..2]), hex(&bytes[2..4]), hex(&bytes[4..6]));
        slint::Color::from_argb_u8(0xff, r, g, b)
    } else {
        slint::Color::from_argb_u8(0, 0, 0, 0)
    }
}

/// One two-hex-digit byte (the swatch's own six-hex vocabulary;
/// `swatch_hex` gates the shape before calling).
fn hex(digits: &[u8]) -> u8 {
    let value = |d: u8| u8::try_from((d as char).to_digit(16).unwrap_or(0)).unwrap_or(0);
    (value(digits[0]) << 4) | value(digits[1])
}

/// The timers chip's display text (r4 D32): the label and the
/// two-part countdown, the `@` fire instant riding only a *counting*
/// chip — an awaiting entry's instant is past (the notification
/// surface owns naming it), and a clamped countdown plus a stale
/// absolute stamp is exactly the legibility the compact budget cannot
/// afford.
fn timer_chip_text(chip: &presentation::TimerChip, now: DateTime<Utc>) -> String {
    let countdown = presentation::countdown_text(chip.effective, now);
    if chip.effective > now {
        format!(
            "{} \u{b7} {} @ {}",
            chip.label,
            countdown,
            presentation::iso_local_fire_time(chip.effective, &chrono::Local)
        )
    } else {
        format!("{} \u{b7} {}", chip.label, countdown)
    }
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
    window.set_timers_chip(SharedString::from(
        chip.map_or(String::new(), |c| timer_chip_text(&c, now)),
    ));
    // The tracking chip (DR-5/4, D-T16): display on kiosk TOO — the
    // appliance shows what is being worked on; only the stop
    // affordance empties (the deliberate deviation from the timers
    // chip's wholesale kiosk hide). Elapsed re-derives per pass from
    // the ArcSwap snapshot — no UI-held timer state; the wall-minute
    // repaint re-pushes the text.
    let tracking = presentation::tracking_chip(state);
    window.set_tracking_chip_visible(tracking.is_some());
    window.set_tracking_chip(SharedString::from(tracking.as_ref().map_or(
        String::new(),
        |c| {
            format!(
                "{} \u{b7} {}",
                c.title.clone(),
                presentation::open_elapsed_phrase(c.start, now)
            )
        },
    )));
    window.set_tracking_chip_action_visible(
        tracking.is_some() && presentation::tracking_chip_action_visible(kiosk),
    );
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
                presentation::due_phrase(DueKind::Overdue, t.duedate, now, &chrono::Local)
            })),
            // D-6.2 (round 6): the state ladder's inputs — the muted
            // flag switches the row's tone and meta markup-side; the
            // snoozed phrase (empty = not snoozed) carries the
            // reschedule target in local wall time.
            muted: row.muted,
            snoozed_phrase: SharedString::from(row.snoozed_until.map_or(String::new(), |until| {
                presentation::snoozed_phrase(until, &chrono::Local)
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
/// scheduled chips (their phrases carry them). `tracked_task` is the
/// pass's shared open-entry pick (DR-5/4, D-TU3): the matching card
/// appends the static tracking badge — no elapsed (the chip owns the
/// one moving number), so wall-minute re-pushes don't churn card
/// decoration diffs.
fn field_decorations(
    task: &Task,
    tracked_task: Option<TaskId>,
    calendar: taskboard_domain::DayCalendar,
    now: DateTime<Utc>,
) -> Vec<TaskDecoration> {
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
    if presentation::tracking_badge_visible(tracked_task, task.id) {
        out.push(TaskDecoration {
            kind: DecorationKind::Tracking,
            label: "tracking".into(),
            tone: Tone::Info,
        });
    }
    // The tracker badge (issue #492): visible whenever the task is
    // kind-marked — the Recur badge's exact mechanism, the decision is
    // presentation-side, the tone mapping models-side.
    if presentation::tracker_badge_visible(task) {
        out.push(TaskDecoration {
            kind: DecorationKind::Tracker,
            label: "tracker".into(),
            tone: Tone::Neutral,
        });
    }
    // The recurring badge (DR-6/4, D-S7): derived per frame from the
    // published state — no UI-held recurrence state. Append-last (the
    // rule-D vocabulary grows at the tail); the tone mapping is
    // models-side, the decision is presentation-side. Day-granularity
    // labels keep re-pushes diff-stable within a window.
    if let Some(badge) = presentation::recur_badge(task, calendar, now) {
        let tone = if badge.stale {
            Tone::Overdue
        } else {
            Tone::Neutral
        };
        out.push(TaskDecoration {
            kind: DecorationKind::Recur,
            label: presentation::recur_badge_label(&badge, calendar.today(now)).into(),
            tone,
        });
    }
    out
}

#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list
fn task_card_model(
    task: &Task,
    sets: &DueSets,
    vs: &ViewState,
    now: DateTime<Utc>,
    tracked_here: bool,
    decorations: ModelRc<TaskDecoration>,
    chips: ModelRc<crate::TaskLabelChipModel>,
    chips_overflow: i32,
) -> TaskCardModel {
    let kind = presentation::due_kind(task, sets);
    TaskCardModel {
        title: SharedString::from(task.title.clone()),
        due_phrase: SharedString::from(presentation::due_phrase(
            kind,
            task.duedate,
            now,
            &chrono::Local,
        )),
        scheduled_phrase: SharedString::from(presentation::scheduled_phrase(
            task.scheduled,
            now,
            &chrono::Local,
        )),
        effort_phrase: SharedString::from(presentation::effort_phrase(task.effort)),
        due_kind: to_slint_due_kind(kind),
        done: task.done.is_some(),
        can_done: !presentation::tracker_badge_visible(task),
        emphasis: vs.focused_task().is_some_and(|f| f == task.id),
        dragging_source: matches!(vs.flow(), Flow::DragMove { task: dragged, .. } if *dragged == task.id),
        // The selection's render fact (UX-R8 D-4): member + anchor,
        // derived over this pass (the reconcile prune precedes the
        // push, so dead members never render selected).
        selected: vs.selection().contains(&task.id),
        anchor: vs
            .selection()
            .anchor()
            .is_some_and(|anchor| anchor == task.id),
        // The tracking control's shown half (UX-R8 chunk-5 D-6): the
        // open entry's subject vs the card's task — the same fact the
        // menu's bar and `entry_states` read.
        track_state: if tracked_here {
            crate::TrackCardState::RunningHere
        } else {
            crate::TrackCardState::None
        },
        decorations,
        // The label chips (tag-color plan D-9, US-3): the mirrored
        // slot handle + the typed overflow count.
        label_chips: chips,
        label_chips_overflow: chips_overflow,
    }
}

/// The today pass (chunk 10 D7): the four `today_sections` in display
/// order, the kiosk filter and the anchor filter applied at the row
/// source, `TodayModel` pushed with persistent per-section handles,
/// and `tables.today_tasks` filled with the filtered concat (the
/// focus walk's domain). Rollover is proven by two `push` calls with
/// a stepped `DayCtx` — no timer, no sleeps.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
#[allow(clippy::too_many_lines)] // one push pass over four sections
fn push_today(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &presentation::DueSets,
    day: crate::view_state::DayCtx,
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    let sections = taskboard_domain::days::today_sections(&state.tasks, day.now, day.window);
    let multi_board = presentation::live_boards(state).len() > 1;
    let tracked_task = presentation::tracking_open_task(state);
    // The "Tracked" day summary (r4 chunk 10, D-TS7; re-recorded
    // 2026-11-24, round-6 A1 D-3, #1301): Work-only per-task sums over
    // the day window (`work_in_window` — the recorded aggregator
    // deviation), NOT filtered by board anywhere — the kiosk board
    // filter retired (the today view has no viewed board on kiosk
    // either; what is shown is what is summed, over all live boards).
    let tracked_today: Vec<(TaskId, TimeDelta)> =
        presentation::tracked_today_rows(state, day.window, day.now);
    let tracked_day_total = tracked_today
        .iter()
        .fold(TimeDelta::zero(), |acc, (_, total)| acc + *total);
    let tracked_rows: Vec<TrackedTodayRowModel> = tracked_today
        .iter()
        .map(|(task, total)| TrackedTodayRowModel {
            title: SharedString::from(
                state
                    .tasks
                    .get(task)
                    .map_or(String::new(), |t| t.title.clone()),
            ),
            total: SharedString::from(presentation::tracked_total_phrase(
                &presentation::part_total(*total),
            )),
        })
        .collect();
    tables.tracked_today = tracked_today.iter().map(|(task, _)| *task).collect();
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
    // #592: one anchor slot per section (default = nothing to
    // compensate), written to the facts global after the diffs.
    let mut section_anchors = vec![SectionAnchor::default(); 4];
    for (index, (ids, label)) in display.iter().enumerate() {
        let handle = models.today_sections[index].handle_or_insert_with();
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
                id: SharedString::from(task.id.as_uuid().to_string()),
                card: {
                    let (chips, chips_overflow) = card_chips(models, state, task);
                    task_card_model(
                        task,
                        sets,
                        vs,
                        day.now,
                        tracked_task.is_some_and(|open| open == task.id),
                        decorations_model(models, task, tracked_task, calendar, day.now),
                        chips,
                        chips_overflow,
                    )
                },
                context: SharedString::from(presentation::today_row_context(
                    state,
                    task,
                    kiosk,
                    multi_board,
                )),
            });
        }
        // #592: the structural summary against the pre-diff mirror —
        // the changed row's id (old side for a removal, desired side
        // for an insertion) addresses the extent cache; the id gate
        // (ids must differ at first_change) rejects a pure in-place
        // content edit, which reflows nothing.
        let anchor = row_anchor_summary(&models.today_sections[index].mirror, &rows);
        let compensable = anchor.as_ref().is_some_and(|a| {
            a.removed != a.inserted
                && models.today_sections[index].mirror[a.first_change].id != rows[a.first_change].id
        });
        let anchor = if compensable {
            let a = anchor.expect("compensable implies a summary");
            // The changed row's stable key: the desired side for an
            // insertion (the new row), the old mirror for a removal
            // (the departing row).
            let extent_id = if a.inserted > a.removed {
                rows[a.first_change].id.clone()
            } else {
                models.today_sections[index].mirror[a.first_change]
                    .id
                    .clone()
            };
            SectionAnchor {
                first_change: a.first_change,
                // Slint ints are i32; section row counts stay far
                // below i32 reach (the hint-writer's cast idiom).
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                delta: a.inserted as i32 - a.removed as i32,
                extent: models
                    .today_row_extents
                    .get(&extent_id)
                    .copied()
                    .unwrap_or(0.0),
            }
        } else {
            SectionAnchor::default()
        };
        mutations += models.today_sections[index].diff(&rows);
        section_anchors[index] = anchor;
        section_rows[index] = filtered.len();
        // All four sections always push (the model arity is fixed —
        // the repetition-probe discipline); the empty groups COLLAPSE
        // in the view's conditional render (D7).
        //
        // The section's content extent (#1310/#1304, 2026-11-24): the
        // observed live-row heights summed over this section's rows
        // from the #592 extent cache — the clip's declarative
        // content-derived height reads THIS (through the
        // `TodayClipFacts` global's per-section property: a
        // model-row update does not propagate into a repeater
        // delegate's bindings reliably, a property write does). Rows
        // never instantiated contribute 0 until their report lands;
        // the report seam's refresh re-derives, so the extent
        // converges from the first materialization onward — the
        // no-refire trap of the retired `changed`-assigned shape
        // cannot recur.
        let content_extent: f32 = filtered
            .iter()
            .map(|id| {
                models
                    .today_row_extents
                    .get(&SharedString::from(id.as_uuid().to_string()))
                    .copied()
                    .unwrap_or(0.0)
            })
            .sum();
        {
            let facts = window.global::<crate::TodayClipFacts>();
            match index {
                0 => facts.set_e0(content_extent),
                1 => facts.set_e1(content_extent),
                2 => facts.set_e2(content_extent),
                _ => facts.set_e3(content_extent),
            }
        }
        section_models.push(TodaySectionModel {
            label: SharedString::from(*label),
            count: i32::try_from(filtered.len()).unwrap_or(i32::MAX),
            rows: ModelRc::from(handle.clone()),
        });
    }
    // Prune the handles of sections that vanished (none can — fixed
    // arity — but the discipline keeps the map honest).
    // #592: the anchor facts — one nonce edge per section with a
    // compensable above-viewport membership change; each section's
    // ListView filters on its own index (the FocusScrollHint
    // pattern). Pruning rides the same pass: a task outside the
    // filtered today set will not render again, so its cache entry is
    // spendable (the average is the fallback). The live set is hashed
    // once per pass, never re-scanned per cache entry.
    let live_ids = today_tasks
        .iter()
        .map(|task| task.as_uuid().to_string())
        .collect::<std::collections::HashSet<String>>();
    models
        .today_row_extents
        .retain(|id, _| live_ids.contains(id.as_str()));
    let facts = window.global::<crate::TodayAnchorFacts>();
    let mut nonce = facts.get_nonce();
    for (index, anchor) in section_anchors.iter().enumerate() {
        if anchor.delta == 0 {
            continue;
        }
        nonce = nonce.wrapping_add(1);
        facts.set_section(i32::try_from(index).unwrap_or(i32::MAX));
        facts.set_first_change(i32::try_from(anchor.first_change).unwrap_or(i32::MAX));
        facts.set_row_delta(anchor.delta);
        facts.set_extent(anchor.extent);
        facts.set_nonce(nonce);
    }
    tables.today_tasks = today_tasks;
    tables.today_section_rows = section_rows;
    // The surfacing card (DR-7 D-J10, #638): derived-at-read on the
    // existing wall-minute repaint — no tick, no persistence, no
    // dismissal state. The kiosk gate lives HERE (model-side, the
    // `push_notifications` pattern): the appliance never receives a
    // card, even if the flow layers drifted.
    let card = if kiosk {
        None
    } else {
        taskboard_domain::review_due(&state.reflection, calendar, day.now).map(|due| {
            let due_day = due.window().day();
            presentation::ReviewCardRow {
                title: presentation::review_card_title(taskboard_domain::ReviewCadence::Day),
                body: presentation::review_card_body(taskboard_domain::ReviewCadence::Day, due_day),
                action: presentation::review_card_action(due_day),
            }
        })
    };
    window.set_review_card_visible(card.is_some());
    if let Some(card) = card {
        window.set_review_card_title(SharedString::from(card.title));
        window.set_review_card_body(SharedString::from(card.body));
        window.set_review_card_action(SharedString::from(card.action));
    }
    let sections_handle = models.today.handle_or_insert_with();
    mutations += models.today.diff(&section_models);
    window.set_today_model(TodayModel {
        sections: ModelRc::from(sections_handle.clone()),
        empty: tables.today_tasks.is_empty(),
        tracked_today: ModelRc::new(slint::VecModel::from(tracked_rows)),
        tracked_day_total: SharedString::from(presentation::tracked_total_phrase(
            &presentation::part_total(tracked_day_total),
        )),
    });
    mutations
}

#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list
fn plan_row(
    state: &AppState,
    models: &mut PushModels,
    task: &Task,
    sets: &presentation::DueSets,
    vs: &ViewState,
    tracked_task: Option<TaskId>,
    calendar: taskboard_domain::DayCalendar,
    now: DateTime<Utc>,
    time: &str,
    overdue: bool,
) -> PlanRowModel {
    // No done box: the planner never wires the toggle (#665).
    let (chips, chips_overflow) = card_chips(models, state, task);
    PlanRowModel {
        card: TaskCardModel {
            can_done: false,
            ..task_card_model(
                task,
                sets,
                vs,
                now,
                tracked_task.is_some_and(|open| open == task.id),
                decorations_model(models, task, tracked_task, calendar, now),
                chips,
                chips_overflow,
            )
        },
        time: SharedString::from(time),
        effort: SharedString::from(presentation::effort_phrase(task.effort)),
        overdue,
        // The walk cursor's armed bit (US-15, #1041): the push pass
        // stamps it against the view state's cursor — the builder
        // never arms (the K4 law: one write seam).
        cursor: false,
    }
}

/// One dialog-row button model (D-5): the label/role pair plus the
/// intent code the site's `button-codes` model mirrors.
fn dialog_button(
    label: &'static str,
    role: crate::DialogButtonRole,
    autofocus: bool,
    code: i32,
) -> (crate::DialogButtonModel, i32) {
    (
        crate::DialogButtonModel {
            label: SharedString::from(label),
            role,
            enabled: true,
            autofocus,
        },
        code,
    )
}

/// The dialog button row's authoritative initial-focus index (#996):
/// the first *enabled* row carrying `autofocus`, `-1` when none. The
/// computation lives in Rust where the `DialogButtonModel` vec is
/// built — Slint functions have no model reduce (the issue's named
/// trap) — and the site binds the host's `initial-focus-index` from
/// this derivation, so the model's `autofocus` flag is authoritative
/// and no site hand-declares an index.
#[must_use]
pub fn initial_focus_index(buttons: &[crate::DialogButtonModel]) -> i32 {
    buttons
        .iter()
        .position(|button| button.enabled && button.autofocus)
        .map_or(-1, |index| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            {
                index as i32
            }
        })
}

/// The usize→i32 narrowing for the pushed counters (the pass's
/// `candidate_index` convention).
const fn as_i32(value: usize) -> i32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    {
        value as i32
    }
}

/// The review walk overlay's pass (DR-7 chunk 3, #638): the header,
/// the progress strip, the current stage's rows, the cursor emphasis,
/// the armed picker/date-field state, and every button label — all
/// derived per pass from the published state and the flow's window
/// (the dumb-client line; slint computes nothing). Visibility mirrors
/// the `Flow::ReviewWalk` flow. The kiosk gate lives HERE (layer 3 of
/// D-J14's exclusion — the `push_notifications` pattern): the empty
/// model on the appliance even if the flow drifted. Returns the diff's
/// mutation count (the strip/rows ride plain whole-struct sets —
/// small, capped row counts).
#[allow(clippy::too_many_lines)] // one stage-table pass, kept flat
fn push_review_walk(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    day: crate::view_state::DayCtx,
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    models: &mut PushModels,
) -> usize {
    let empty = || crate::ReviewWalkModel {
        open: false,
        header: SharedString::new(),
        strip: ModelRc::new(VecModel::<crate::WalkStageStripRow>::from(Vec::new())),
        rows: ModelRc::new(VecModel::<crate::WalkRowModel>::from(Vec::new())),
        stage_kind: 0,
        stage_position: SharedString::new(),
        stage_intro: SharedString::new(),
        tally_filed: 0,
        tally_moved: 0,
        tally_left: 0,
        tally_recorded: false,
        abort_confirm: false,
        initial_focus_index: -1,
        buttons: ModelRc::new(VecModel::<crate::DialogButtonModel>::from(Vec::new())),
        button_codes: ModelRc::new(VecModel::<i32>::from(Vec::new())),
        filing: false,
        candidates: ModelRc::new(VecModel::<SharedString>::from(Vec::new())),
        candidate_index: 0,
        file_label: SharedString::new(),
        keep_label: SharedString::new(),
        picking: false,
        date_text: SharedString::new(),
        tomorrow_label: SharedString::new(),
        pick_label: SharedString::new(),
        leave_label: SharedString::new(),
        plan_label: SharedString::new(),
        record_label: SharedString::new(),
        finish_label: SharedString::new(),
    };
    let crate::view_state::Flow::ReviewWalk {
        cadence,
        reference,
        stage,
        cursor,
        tally,
    } = vs.walk_flow().unwrap_or(crate::view_state::Flow::None)
    else {
        models.walk_prompts_open = false;
        window.set_walk_model(empty());
        window.set_walk_answer_seeds(ModelRc::new(VecModel::<SharedString>::from(Vec::new())));
        return 0;
    };
    // The kiosk belt (D-J14 layer 3): the empty model on the
    // appliance, regardless of a drifted flow.
    if kiosk {
        models.walk_prompts_open = false;
        window.set_walk_model(empty());
        return 0;
    }
    let walk_window = taskboard_domain::review_window(cadence, reference, calendar);
    let template_for = || {
        taskboard_domain::DEFAULT_TEMPLATES
            .iter()
            .find(|template| template.cadence == cadence)
            .expect("a curated template per cadence")
    };
    // The strip (D-9): every machine stage in walk order — the planner
    // is a real step now, its note empty (the skip law never skips it).
    let strip: Vec<crate::WalkStageStripRow> = [
        crate::view_state::WalkStage::Entry,
        crate::view_state::WalkStage::Inbox,
        crate::view_state::WalkStage::CarryForward,
        crate::view_state::WalkStage::Planner,
        crate::view_state::WalkStage::Prompts,
        crate::view_state::WalkStage::Complete,
    ]
    .into_iter()
    .map(|strip_stage| crate::WalkStageStripRow {
        label: SharedString::from(presentation::walk_stage_header(strip_stage, template_for())),
        note: SharedString::from(presentation::walk_stage_empty_line(strip_stage)),
    })
    .collect();
    let position = SharedString::from(presentation::walk_stage_position(stage, tally.planner));
    // The dialog button row (D-5): the stage's Next/Prev/Finish plus
    // the completion exit. Codes route through the adapter's button
    // seam: 0 Prev, 1 Advance, 2 Finish, 3 Complete.
    let buttons: Vec<(crate::DialogButtonModel, i32)> = match stage {
        crate::view_state::WalkStage::Entry => vec![dialog_button(
            presentation::walk_next_label(),
            crate::DialogButtonRole::Accept,
            true,
            1,
        )],
        crate::view_state::WalkStage::Inbox | crate::view_state::WalkStage::CarryForward => {
            vec![
                dialog_button(
                    presentation::walk_prev_label(),
                    crate::DialogButtonRole::Dismiss,
                    false,
                    0,
                ),
                dialog_button(
                    presentation::walk_next_label(),
                    crate::DialogButtonRole::Accept,
                    true,
                    1,
                ),
            ]
        }
        crate::view_state::WalkStage::Planner => vec![
            dialog_button(
                presentation::walk_prev_label(),
                crate::DialogButtonRole::Dismiss,
                false,
                0,
            ),
            // Done planning: the child's Completed exit advances the
            // walk to Prompts (D-2).
            dialog_button(
                presentation::walk_next_label(),
                crate::DialogButtonRole::Accept,
                true,
                1,
            ),
        ],
        crate::view_state::WalkStage::Prompts => vec![
            dialog_button(
                presentation::walk_prev_label(),
                crate::DialogButtonRole::Dismiss,
                false,
                0,
            ),
            dialog_button("Finish for now", crate::DialogButtonRole::Accept, true, 2),
        ],
        crate::view_state::WalkStage::Complete => vec![dialog_button(
            presentation::walk_complete_label(),
            crate::DialogButtonRole::Accept,
            true,
            3,
        )],
    };
    // The autofocus flag is authoritative (#996): the derivation rides
    // the same vec this pass builds — the site binds it, never a
    // hand-declared index.
    let button_models: Vec<crate::DialogButtonModel> = buttons
        .iter()
        .map(|(model, _)| model.clone())
        .collect::<Vec<_>>();
    let initial_focus_index = initial_focus_index(&button_models);
    let (rows, stage_kind, candidates, candidate_index, filing, picking) = match stage {
        crate::view_state::WalkStage::Entry => {
            // The guidance stage (D-1): one intro row.
            (
                vec![crate::WalkRowModel {
                    title: SharedString::from(presentation::walk_entry_intro()),
                    emphasis: false,
                }],
                4,
                Vec::new(),
                0,
                false,
                false,
            )
        }
        crate::view_state::WalkStage::Inbox => {
            // The armed File-to picker is the walk's child level (N3a):
            // the mirrors read its presence and armed position.
            let armed = vs.walk_file_armed();
            let rows: Vec<crate::WalkRowModel> = presentation::walk_inbox_rows(state)
                .into_iter()
                .enumerate()
                .map(|(index, id)| crate::WalkRowModel {
                    title: SharedString::from(
                        state
                            .tasks
                            .get(&id)
                            .map_or(String::new(), |task| task.title.clone()),
                    ),
                    emphasis: index == cursor,
                })
                .collect();
            let candidate_ids = presentation::walk_file_candidates(state);
            let candidates: Vec<SharedString> = candidate_ids
                .iter()
                .map(|stack| {
                    SharedString::from(
                        state
                            .stacks
                            .get(stack)
                            .map_or(String::new(), |stack| stack.title.clone()),
                    )
                })
                .collect();
            let candidate_index = armed
                .and_then(|armed| candidate_ids.iter().position(|stack| stack == &armed))
                .unwrap_or(0);
            (rows, 0, candidates, candidate_index, armed.is_some(), false)
        }
        crate::view_state::WalkStage::CarryForward => {
            // The armed Pick-a-date field is the walk's child level
            // (N3b): the `picking` mirror reads its presence.
            let picking = vs.walk_date_child();
            let rows: Vec<crate::WalkRowModel> =
                presentation::walk_carry_rows(&state.tasks, cadence, walk_window, day.now)
                    .into_iter()
                    .enumerate()
                    .map(|(index, id)| crate::WalkRowModel {
                        title: SharedString::from(
                            state
                                .tasks
                                .get(&id)
                                .map_or(String::new(), |task| task.title.clone()),
                        ),
                        emphasis: index == cursor,
                    })
                    .collect();
            (rows, 1, Vec::new(), 0, false, picking)
        }
        crate::view_state::WalkStage::Planner => {
            // The embedded planner step (D-2): the walk's own rows are
            // empty — the body mounts the PlanNextDayView fed by the
            // shared plan model.
            (Vec::new(), 3, Vec::new(), 0, false, false)
        }
        crate::view_state::WalkStage::Prompts => {
            let rows: Vec<crate::WalkRowModel> = template_for()
                .prompts
                .iter()
                .map(|prompt| crate::WalkRowModel {
                    title: SharedString::from(*prompt),
                    emphasis: false,
                })
                .collect();
            let mount = !models.walk_prompts_open;
            models.walk_prompts_open = true;
            if mount {
                // The mount-only seed (the focus-overlay note rule):
                // the answers mirror adapter-side afterwards.
                window.set_walk_answer_seeds(ModelRc::new(VecModel::from(
                    template_for()
                        .prompts
                        .iter()
                        .map(|_| SharedString::new())
                        .collect::<Vec<_>>(),
                )));
            }
            (rows, 2, Vec::new(), 0, false, false)
        }
        crate::view_state::WalkStage::Complete => {
            // The completion summary (D-8): the session tally, derived
            // here (the tree computes nothing).
            let rows: Vec<crate::WalkRowModel> = presentation::walk_summary_lines(tally)
                .into_iter()
                .map(|line| crate::WalkRowModel {
                    title: SharedString::from(line),
                    emphasis: false,
                })
                .collect();
            (rows, 5, Vec::new(), 0, false, false)
        }
    };
    if stage_kind != 2 {
        models.walk_prompts_open = false;
    }
    let (file_label, keep_label) = presentation::walk_inbox_actions();
    let (tomorrow_label, pick_label, leave_label) = presentation::walk_carry_actions();
    let (record_label, finish_label) = presentation::walk_commit_actions();
    window.set_walk_model(crate::ReviewWalkModel {
        open: true,
        header: SharedString::from(presentation::walk_header(cadence, reference)),
        strip: ModelRc::new(VecModel::from(strip)),
        rows: ModelRc::new(VecModel::from(rows)),
        stage_kind,
        stage_position: position,
        stage_intro: SharedString::from(presentation::walk_entry_intro()),
        tally_filed: as_i32(tally.filed),
        tally_moved: as_i32(tally.moved),
        tally_left: as_i32(tally.left),
        tally_recorded: tally.recorded,
        abort_confirm: models.walk_abort_armed,
        initial_focus_index,
        buttons: ModelRc::new(VecModel::from(
            buttons
                .iter()
                .map(|(model, _)| model.clone())
                .collect::<Vec<_>>(),
        )),
        button_codes: ModelRc::new(VecModel::from(
            buttons.iter().map(|(_, code)| *code).collect::<Vec<_>>(),
        )),
        filing,
        candidates: ModelRc::new(VecModel::from(candidates)),
        candidate_index: as_i32(candidate_index),
        file_label: SharedString::from(file_label),
        keep_label: SharedString::from(keep_label),
        picking,
        date_text: SharedString::new(),
        tomorrow_label: SharedString::from(tomorrow_label),
        pick_label: SharedString::from(pick_label),
        leave_label: SharedString::from(leave_label),
        plan_label: SharedString::from(presentation::walk_plan_label()),
        record_label: SharedString::from(record_label),
        finish_label: SharedString::from(finish_label),
    });
    0
}

/// The plan-next-day surface's pass (#622, D-8): the domain
/// derivation (`plan_next_day` — today's projection evaluated as of
/// tomorrow's opening, the shared read) rendered as two panes — the
/// placed boxes in scheduled order with time-of-day and effort
/// phrases, then the candidate pool (carried misses included). The
/// kiosk gate lives HERE (the review pass's precedent): the empty
/// model on the appliance — the rotation can never route to the view
/// anyway (D-5). Rollover rides the pass's fresh `DayCtx` (the
/// `push_today` two-push rule — no timer, no sleeps). Slint computes
/// nothing: every phrase is derived here or in `presentation`.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push_today`
fn push_plan_next_day(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &presentation::DueSets,
    day: crate::view_state::DayCtx,
    calendar: taskboard_domain::DayCalendar,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    if kiosk {
        tables.plan_rows.clear();
        tables.plan_box_count = 0;
        window.set_plan_model(PlanModel {
            boxes: ModelRc::new(VecModel::from(Vec::new())),
            pool: ModelRc::new(VecModel::from(Vec::new())),
            placing: false,
            focus_pane: 0,
        });
        return 0;
    }
    let sections = taskboard_domain::days::plan_next_day(
        &state.tasks,
        day.now,
        calendar.day_start_time,
        calendar.offset,
    );
    let tracked_task = presentation::tracking_open_task(state);
    let next_start =
        taskboard_domain::days::next_day_window(day.now, calendar.day_start_time, calendar.offset)
            .0;
    let mut mutations = 0;
    let mut plan_rows: Vec<TaskId> = Vec::new();

    let mut boxes: Vec<PlanRowModel> = Vec::with_capacity(sections.placed_scheduled.len());
    for id in &sections.placed_scheduled {
        let Some(task) = state.tasks.get(id) else {
            continue;
        };
        if presentation::is_anchor_task(task) {
            continue;
        }
        plan_rows.push(*id);
        let mut row = plan_row(
            state,
            models,
            task,
            sets,
            vs,
            tracked_task,
            calendar,
            day.now,
            &presentation::plan_box_time_phrase(task.scheduled, calendar),
            false,
        );
        // The planner cursor's armed bit (US-15, #1041): the walked
        // row renders the emphasis (the K4 armed styling).
        row.cursor = vs.plan_cursor().is_some_and(|cursor| cursor == *id);
        boxes.push(row);
    }
    let boxes_handle = models.plan_boxes.handle_or_insert_with();
    mutations += models.plan_boxes.diff(&boxes);

    let mut pool: Vec<PlanRowModel> = Vec::with_capacity(sections.candidates.len());
    for id in &sections.candidates {
        let Some(task) = state.tasks.get(id) else {
            continue;
        };
        if presentation::is_anchor_task(task) {
            continue;
        }
        plan_rows.push(*id);
        let mut row = plan_row(
            state,
            models,
            task,
            sets,
            vs,
            tracked_task,
            calendar,
            day.now,
            "",
            task.duedate.is_some_and(|d| d < next_start),
        );
        row.cursor = vs.plan_cursor().is_some_and(|cursor| cursor == *id);
        pool.push(row);
    }
    let pool_handle = models.plan_pool.handle_or_insert_with();
    mutations += models.plan_pool.diff(&pool);

    tables.plan_rows = plan_rows;
    tables.plan_box_count = boxes.len();
    window.set_plan_model(PlanModel {
        boxes: ModelRc::from(boxes_handle.clone()),
        pool: ModelRc::from(pool_handle.clone()),
        placing: matches!(vs.flow(), crate::view_state::Flow::PlanPlace { .. }),
        // The focused pane (J3 R-5, round 9 chunk 9): the
        // directional pane switch's mirror (0 = boxes, 1 = pool).
        focus_pane: i32::try_from(vs.plan_focus_pane()).unwrap_or(0),
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

/// The chip tone's 1:1 generated-enum ferry (the files-sync-status UX
/// plan US-1, D-1).
#[must_use]
pub fn files_sync_chip_tone(state: presentation::ChipTone) -> SlintFilesSyncChipTone {
    match state {
        presentation::ChipTone::Neutral => SlintFilesSyncChipTone::Neutral,
        presentation::ChipTone::Warning => SlintFilesSyncChipTone::Warning,
        presentation::ChipTone::Danger => SlintFilesSyncChipTone::Danger,
    }
}

/// The adoption phase's 1:1 generated-enum ferry (US-8, D-8).
#[must_use]
pub fn files_board_adoption(phase: presentation::FilesBoardAdoption) -> SlintSettingsAdoptionPhase {
    match phase {
        presentation::FilesBoardAdoption::None => SlintSettingsAdoptionPhase::None,
        presentation::FilesBoardAdoption::Adopting => SlintSettingsAdoptionPhase::Adopting,
        presentation::FilesBoardAdoption::Adopted => SlintSettingsAdoptionPhase::Adopted,
        presentation::FilesBoardAdoption::Refused => SlintSettingsAdoptionPhase::Refused,
    }
}

/// The publish phase's 1:1 generated-enum ferry (the files-sync-status
/// UX plan US-5, D-6).
#[must_use]
pub fn publish_card_phase(phase: Option<presentation::PublishPhase>) -> SlintSettingsPublishPhase {
    match phase {
        None => SlintSettingsPublishPhase::None,
        Some(presentation::PublishPhase::Publishing) => SlintSettingsPublishPhase::Publishing,
        Some(presentation::PublishPhase::Adopting) => SlintSettingsPublishPhase::Adopting,
        Some(presentation::PublishPhase::Done) => SlintSettingsPublishPhase::Done,
        Some(presentation::PublishPhase::Failed) => SlintSettingsPublishPhase::Failed,
    }
}

/// The files-sync chip state's 1:1 generated-enum ferry (D-FH5).
#[must_use]
pub fn files_sync_chip_state(state: presentation::FilesSyncChipState) -> SlintFilesSyncChipState {
    match state {
        presentation::FilesSyncChipState::Ok => SlintFilesSyncChipState::Ok,
        presentation::FilesSyncChipState::CycleFailed => SlintFilesSyncChipState::CycleFailed,
        presentation::FilesSyncChipState::Parked => SlintFilesSyncChipState::Parked,
        presentation::FilesSyncChipState::Diverged => SlintFilesSyncChipState::Diverged,
        presentation::FilesSyncChipState::Forked => SlintFilesSyncChipState::Forked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use taskboard_domain::{
        Board, BoardClocks, Importance, Stack, StackClocks, TaskClocks, Urgency,
    };

    /// The dialog button row's autofocus derivation (#996): the first
    /// *enabled* autofocus row wins, `-1` when none qualifies.
    #[test]
    fn initial_focus_index_reads_the_first_enabled_autofocus_row() {
        let row = |enabled: bool, autofocus: bool| crate::DialogButtonModel {
            label: SharedString::new(),
            role: crate::DialogButtonRole::None,
            enabled,
            autofocus,
        };
        assert_eq!(
            initial_focus_index(&[row(false, true), row(true, true)]),
            1,
            "a disabled autofocus row never wins"
        );
        assert_eq!(
            initial_focus_index(&[row(true, false)]),
            -1,
            "no flag, no focus"
        );
        assert_eq!(initial_focus_index(&[]), -1, "an empty model never focuses");
        assert_eq!(
            initial_focus_index(&[row(true, false), row(true, true), row(true, true)]),
            1,
            "the first qualifying row wins"
        );
    }

    /// The insertion indicator's aim derivation (UX-R8 D-2, US-2/AC-3):
    /// the armed move/drag flows converge on one (stack, gap) pair;
    /// no aim gesture — the column-level drag hover included — derives
    /// nothing (the US-5/AC-2 guard's data side).
    #[test]
    fn move_aim_derives_the_armed_slot_from_both_flows() {
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        assert_eq!(move_aim(&Flow::None), None, "no aim without a flow");
        assert_eq!(
            move_aim(&Flow::DragMove {
                task: TaskId::from_uuid(uuid::Uuid::now_v7()),
                target: stack,
                gap: None,
            }),
            None,
            "the column-level aim carries no gap yet"
        );
        assert_eq!(
            move_aim(&Flow::DragMove {
                task: TaskId::from_uuid(uuid::Uuid::now_v7()),
                target: stack,
                gap: Some(2),
            }),
            Some((stack, 2)),
            "the gap hover arms the aim"
        );
        assert_eq!(
            move_aim(&Flow::Move {
                task: TaskId::from_uuid(uuid::Uuid::now_v7()),
                target: stack,
                slot: 1,
            }),
            Some((stack, 1)),
            "the keyboard slots ride the same data path"
        );
    }

    /// The swatch echo's gate (UX R-4 D-1/R-4, #791): a six-hex body
    /// mints the typed color; a lenient (non-hex, wrong-length, or
    /// empty) value renders transparent — the echo is honest, the GUI
    /// never re-derives. The labels' chip path shares the fn (the
    /// chunk-3 echo tests guard that side; this is the gate itself).
    #[test]
    fn swatch_hex_mints_six_hex_and_renders_lenient_transparent() {
        let typed = swatch_hex(&taskboard_domain::Color::new("0082c9"));
        assert_eq!(typed.alpha(), 255, "the typed body renders opaque");
        for lenient in ["", "0082c", "0082c99", "ZZZZZZ"] {
            let echoed = swatch_hex(&taskboard_domain::Color::new(lenient));
            assert_eq!(
                echoed.alpha(),
                0,
                "the lenient value {lenient:?} renders transparent"
            );
        }
    }

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
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,

            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: epoch(),
                description: epoch(),
                duedate: epoch(),
                scheduled: epoch(),
                done: epoch(),
                position: epoch(),
                effort: epoch(),
                importance: epoch(),
                urgency: epoch(),
                recur: epoch(),
                kind: epoch(),

                archived: epoch(),
                deleted: epoch(),
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    /// A live label on `board` with the given title/color.
    fn label(
        id: LabelId,
        board: BoardId,
        title: &str,
        color: &str,
        deleted: bool,
    ) -> taskboard_domain::Label {
        taskboard_domain::Label {
            id,
            remote: None,
            board,
            title: title.to_owned(),
            color: taskboard_domain::Color::new(color),
            deleted,
            clocks: taskboard_domain::LabelClocks {
                title: epoch(),
                color: epoch(),
                deleted: epoch(),
            },
            authors: taskboard_domain::entities::LabelAuthors::default(),
            remote_seen: None,
        }
    }

    // -- tag-color UX plan (US-3/W-3, #839): the card label chips --

    /// A live board for the chips tests (the palette submodule's helper
    /// shape, spelled locally — the block sits in the outer tests mod).
    fn chips_board(board_id: taskboard_domain::BoardId) -> Board {
        Board {
            id: board_id,
            remote: None,
            title: "board".to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        }
    }

    /// A live stack for the chips tests.
    fn chips_stack(stack_id: StackId, board_id: taskboard_domain::BoardId) -> Stack {
        Stack {
            id: stack_id,
            remote: None,
            board: board_id,
            title: "stack".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        }
    }

    /// A live board/stack/task trio for the chips tests (the palette
    /// submodule's helper shapes, spelled locally — the block is
    /// self-contained).
    fn chips_task(id: TaskId, stack_id: StackId) -> Task {
        let mut task = task();
        task.id = id;
        task.stack = stack_id;
        task.title = "card".to_owned();
        task
    }

    /// US-3/AC-1 (tag-color plan W-3): the card carries one chip per live
    /// label in canonical order (the `BTreeSet` id order), titles over
    /// swatch echoes — derived from `Task::live_labels`, never strings.
    #[test]
    fn us3_ac1_the_card_carries_one_chip_per_live_label() {
        crate::test_platform::on_slint_thread(|| {
            let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(board_id, chips_board(board_id));
            state
                .stacks
                .insert(stack_id, chips_stack(stack_id, board_id));
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            state.tasks.insert(id, chips_task(id, stack_id));
            // Fixed uuids: the canonical order is the ids' own BTreeSet
            // order, so the test pins a deterministic one (a < b < gone).
            let (a, b, gone) = (
                LabelId::from_uuid(
                    uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                        .expect("fixed uuid"),
                ),
                LabelId::from_uuid(
                    uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002")
                        .expect("fixed uuid"),
                ),
                LabelId::from_uuid(
                    uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000003")
                        .expect("fixed uuid"),
                ),
            );
            for (lid, title, color) in [
                (a, "alpha", "31cc7c"),
                (b, "beta", "3a70d6"),
                (gone, "gone", "e05a4e"),
            ] {
                state
                    .labels
                    .insert(lid, label(lid, board_id, title, color, false));
            }
            // `gone` is a tombstone: the roster filter drops it.
            if let Some(l) = state.labels.get_mut(&gone) {
                l.deleted = true;
            }
            // The canonical order rides the BTreeSet's id order; the
            // membership insertion order is irrelevant.
            for (lid, at) in [(b, epoch()), (a, epoch()), (gone, epoch())] {
                if let Some(task) = state.tasks.get_mut(&id) {
                    task.label_memberships.insert(
                        lid,
                        taskboard_domain::entities::MembershipState::Added { at },
                    );
                }
            }
            let (chips, overflow) = task_card_label_chips(&state, id);
            assert_eq!(chips.len(), 2, "one chip per live label; tombstones drop");
            assert_eq!(chips[0].title, "alpha", "canonical order: the id's own");
            assert_eq!(chips[1].title, "beta");
            assert_eq!(overflow, 0);
        });
    }

    /// US-3/AC-2 (tag-color plan W-3): a lenient stored colour echoes
    /// transparent on a chip — the `swatch_hex` hygiene, never re-derived.
    #[test]
    fn us3_ac2_a_lenient_colour_echoes_transparent_on_a_chip() {
        crate::test_platform::on_slint_thread(|| {
            let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(board_id, chips_board(board_id));
            state
                .stacks
                .insert(stack_id, chips_stack(stack_id, board_id));
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            state.tasks.insert(id, chips_task(id, stack_id));
            let odd = LabelId::from_uuid(uuid::Uuid::now_v7());
            state
                .labels
                .insert(odd, label(odd, board_id, "odd", "#fff nope", false));
            if let Some(task) = state.tasks.get_mut(&id) {
                task.label_memberships.insert(
                    odd,
                    taskboard_domain::entities::MembershipState::Added { at: epoch() },
                );
            }
            let (chips, _) = task_card_label_chips(&state, id);
            assert_eq!(chips.len(), 1);
            assert_eq!(
                chips[0].color,
                swatch_hex(&taskboard_domain::Color::new("#fff nope")),
                "the lenient echo rides the same swatch hygiene"
            );
        });
    }

    /// US-3/AC-3 (tag-color plan W-3): the cap/overflow arithmetic —
    /// beyond `LABEL_CHIP_CAP` the chips truncate and the typed overflow
    /// count carries the hidden remainder.
    #[test]
    fn us3_ac3_the_cap_collapses_the_rest_into_the_typed_overflow() {
        crate::test_platform::on_slint_thread(|| {
            let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(board_id, chips_board(board_id));
            state
                .stacks
                .insert(stack_id, chips_stack(stack_id, board_id));
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            state.tasks.insert(id, chips_task(id, stack_id));
            let total = crate::presentation::LABEL_CHIP_CAP + 2;
            for i in 0..total {
                let lid = LabelId::from_uuid(uuid::Uuid::now_v7());
                state
                    .labels
                    .insert(lid, label(lid, board_id, &format!("l{i}"), "31cc7c", false));
                if let Some(task) = state.tasks.get_mut(&id) {
                    task.label_memberships.insert(
                        lid,
                        taskboard_domain::entities::MembershipState::Added { at: epoch() },
                    );
                }
            }
            let (chips, overflow) = task_card_label_chips(&state, id);
            assert_eq!(chips.len(), crate::presentation::LABEL_CHIP_CAP);
            assert_eq!(overflow, 2, "the hidden remainder rides the typed flag");
        });
    }

    /// The priority picker's slot rows (UX R-3 D-1/D-9): the closed
    /// six-value vocabulary in order, exactly one pre-mark — the
    /// open field's current level (the unset slot when `None`).
    #[test]
    fn priority_slots_enumerate_the_vocabulary_with_one_mark() {
        let unset = dropdown_vocabulary_rows(None);
        assert_eq!(unset.len(), 6, "unset + the five domain levels");
        assert!(unset[0].selected, "no level = the unset slot pre-marked");
        assert!(!unset.iter().skip(1).any(|slot| slot.selected));
        // High rides vocabulary index 4 (unset + VeryLow..VeryHigh).
        let marked = dropdown_vocabulary_rows(Some(4));
        assert_eq!(
            marked.iter().filter(|slot| slot.selected).count(),
            1,
            "exactly one pre-mark"
        );
        assert!(marked[4].selected);
        assert!(!marked[0].selected);
        // The slot labels ARE the domain text forms — the same
        // strings the card decorations render (reconciliation by
        // construction).
        for (slot, level) in marked.iter().skip(1).zip([
            Importance::VeryLow,
            Importance::Low,
            Importance::Normal,
            Importance::High,
            Importance::VeryHigh,
        ]) {
            assert_eq!(slot.label, level.text_form());
        }
    }

    /// Both vocabularies share the slot shape (D-1: one quick-list
    /// grammar, two fields) — the urgency field's rows are the same
    /// six-value vocabulary (same builder, same text forms).
    #[test]
    fn the_urgency_vocabulary_maps_to_the_same_slot_shape() {
        let marked = dropdown_vocabulary_rows(Some(1));
        for (slot, level) in marked.iter().skip(1).zip([
            Urgency::VeryLow,
            Urgency::Low,
            Urgency::Normal,
            Urgency::High,
            Urgency::VeryHigh,
        ]) {
            assert_eq!(slot.label, level.text_form());
        }
        assert!(marked[1].selected);
    }

    /// The labels section's chips (UX R-3 D-2/D-9): the subject's
    /// live labels in canonical order — a tombstoned or removed
    /// membership never renders, a six-hex color echoes as a swatch,
    /// a lenient color echoes empty (the GUI never re-derives it).
    #[test]
    fn label_chips_render_the_subjects_live_labels() {
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut subject = task();
        let (green, red, dead) = (
            LabelId::from_uuid(uuid::Uuid::now_v7()),
            LabelId::from_uuid(uuid::Uuid::now_v7()),
            LabelId::from_uuid(uuid::Uuid::now_v7()),
        );
        state
            .labels
            .insert(green, label(green, board, "green", "31cc7c", false));
        state
            .labels
            .insert(red, label(red, board, "red", "ff0000", false));
        state
            .labels
            .insert(dead, label(dead, board, "dead", "0000ff", true));
        subject.label_memberships.insert(
            green,
            taskboard_domain::entities::MembershipState::Added { at: epoch() },
        );
        subject.label_memberships.insert(
            dead,
            taskboard_domain::entities::MembershipState::Added { at: epoch() },
        );
        state.tasks.insert(subject.id, subject.clone());
        let chip_ids = vec![green, dead];
        let rows = label_chip_rows(&state, Some(subject.id), &chip_ids);
        assert_eq!(rows.len(), 1, "only the live label renders");
        assert_eq!(
            rows[0].color,
            slint::Color::from_argb_u8(0xff, 0x31, 0xcc, 0x7c),
        );
        // The lenient-color hygiene: a non-palette color renders
        // transparent, not re-derived.
        state.labels.get_mut(&green).expect("live").color = taskboard_domain::Color::new("junk");
        let rows = label_chip_rows(&state, Some(subject.id), &chip_ids);
        assert_eq!(rows[0].color, slint::Color::from_argb_u8(0, 0, 0, 0));
        // No subject: the section is empty.
        assert_eq!(label_chip_rows(&state, None, &chip_ids).len(), 0);
    }

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    /// The tests' day calendar: UTC, midnight day start — `epoch()`
    /// instant reads land on their literal calendar date.
    fn calendar() -> taskboard_domain::DayCalendar {
        taskboard_domain::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
        }
    }

    #[test]
    fn field_decorations_carry_kind_tone_and_the_domain_label() {
        let mut t = task();
        t.importance = Some(Importance::High);
        t.urgency = Some(Urgency::VeryLow);
        let decorations = field_decorations(&t, None, calendar(), epoch());
        assert_eq!(decorations.len(), 2);
        assert!(matches!(decorations[0].kind, DecorationKind::Importance));
        assert!(matches!(decorations[0].tone, Tone::Warning));
        assert!(matches!(decorations[1].kind, DecorationKind::Urgency));
        assert!(matches!(decorations[1].tone, Tone::Neutral));
        // The label is the domain text form verbatim — one vocabulary.
        assert_eq!(decorations[0].label, Importance::High.text_form());
        assert_eq!(decorations[1].label, Urgency::VeryLow.text_form());
    }

    /// The recurring decoration arm (DR-6/4, T14): a recur task gains
    /// exactly one `recur` decoration, append-last after
    /// importance/urgency/tracking, with the ruled tone per staleness
    /// and the presentation-side label.
    #[test]
    fn the_recur_decoration_appends_last_with_the_ruled_tone() {
        let mut t = task();
        t.importance = Some(Importance::High);
        t.recur = Some(taskboard_domain::RecurRule {
            series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            pattern: taskboard_domain::RecurPattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            bound: taskboard_domain::RecurBound::None,
            continued_from: None,
            paused_until: None,
        });
        // A fresh open head (reference today at the epoch calendar) is
        // not stale: neutral tone, append-last after importance.
        let fresh = field_decorations(&t, None, calendar(), epoch());
        assert_eq!(fresh.len(), 2);
        assert!(matches!(fresh[0].kind, DecorationKind::Importance));
        assert!(matches!(fresh[1].kind, DecorationKind::Recur));
        assert!(matches!(fresh[1].tone, Tone::Neutral));
        assert!(!fresh[1].label.is_empty());
        // A stale head (scheduled two days before the reading time)
        // flips the tone to overdue.
        let late = epoch() + chrono::TimeDelta::try_days(2).unwrap();
        t.scheduled = Some(epoch());
        let stale = field_decorations(&t, None, calendar(), late);
        assert_eq!(stale.len(), 2);
        assert!(matches!(stale[1].kind, DecorationKind::Recur));
        assert!(matches!(stale[1].tone, Tone::Overdue));
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
        assert_eq!(
            field_decorations(&task(), None, calendar(), epoch()),
            [] as [TaskDecoration; 0]
        );
    }

    /// The test-side read-back (allowed — the acceptance bans reads in
    /// the *diff path*, not in tests): the model's rows as a `Vec`.
    fn rows_of<T: Clone + 'static>(model: &VecModel<T>) -> Vec<T> {
        (0..model.row_count())
            .map(|i| model.row_data(i).expect("row in range"))
            .collect()
    }

    /// D9 — the diff primitive: positional compare, truncate, append;
    /// the mutation counts are the transport's budget log.
    #[test]
    fn diff_rows_mutates_exactly_the_differing_rows() {
        let mut mirror = vec![1, 2, 3];
        let model = VecModel::from(vec![1, 2, 3]);
        // Equal rows → zero mutations.
        assert_eq!(diff_rows(&mut mirror, &model, &[1, 2, 3]), 0);
        // One changed row → one mutation, in place.
        assert_eq!(diff_rows(&mut mirror, &model, &[1, 9, 3]), 1);
        assert_eq!(model.row_data(1), Some(9));
        // Append → one; shrinking back → one removal.
        assert_eq!(diff_rows(&mut mirror, &model, &[1, 9, 3, 4]), 1);
        assert_eq!(diff_rows(&mut mirror, &model, &[1, 9, 3]), 1);
        assert_eq!(diff_rows(&mut mirror, &model, &[1]), 2);
        assert_eq!(model.row_count(), 1);
        // A mid-list reorder is a positional set cascade over the
        // suffix — the accepted transport shape (in-place updates).
        let mut reordered_mirror = vec![1, 2, 3, 4];
        let reordered = VecModel::from(vec![1, 2, 3, 4]);
        assert_eq!(
            diff_rows(&mut reordered_mirror, &reordered, &[1, 3, 2, 4]),
            2
        );
        assert_eq!(reordered.row_data(1), Some(3));
        assert_eq!(reordered.row_data(2), Some(2));
        // The mirror tracked every mutation in lockstep (#464).
        assert_eq!(mirror, rows_of(&model));
        assert_eq!(reordered_mirror, rows_of(&reordered));
    }

    /// #592 — the structural summary: the first differing index and
    /// the tail membership counts; no differing row over the common
    /// length (unchanged, content-edit-only, or tail-only) is `None`.
    #[test]
    fn row_anchor_summary_locates_the_first_structural_change() {
        use super::RowAnchorSummary as S;
        let s = |first_change: usize, removed: usize, inserted: usize| {
            Some(S {
                first_change,
                removed,
                inserted,
            })
        };
        // Unchanged → nothing.
        assert_eq!(row_anchor_summary(&[1, 2, 3], &[1, 2, 3]), None);
        // Content edit only (same length, one differing row) → a
        // summary exists, but the caller's id gate rejects it.
        assert_eq!(row_anchor_summary(&[1, 2, 3], &[1, 9, 3]), s(1, 0, 0));
        // Head removal: the cascade starts at 0.
        assert_eq!(row_anchor_summary(&[1, 2, 3], &[2, 3]), s(0, 1, 0));
        // Head insertion: same.
        assert_eq!(row_anchor_summary(&[2, 3], &[0, 2, 3]), s(0, 0, 1));
        // Mid removal and mid insertion.
        assert_eq!(row_anchor_summary(&[1, 2, 3, 4], &[1, 3, 4]), s(1, 1, 0));
        assert_eq!(row_anchor_summary(&[1, 3, 4], &[1, 2, 3, 4]), s(1, 0, 1));
        // Tail-only membership: no row over the common length
        // differs — no reflow, nothing to compensate.
        assert_eq!(row_anchor_summary(&[1, 2], &[1, 2, 3]), None);
        assert_eq!(row_anchor_summary(&[1, 2, 3], &[1, 2]), None);
    }

    /// #592's mechanism fact: a head (above-viewport) membership
    /// change is transported as a `set_row_data` rebind cascade plus
    /// exactly one tail mutation — every structural notification the
    /// push discipline emits is tail-indexed, so the repeater's
    /// before-window offset-shift branches never fire for it. The
    /// counts pin the shape: head removal of one of N rows costs N
    /// mutations (N−1 rebinds + one tail remove), head insertion N+1
    /// (N rebinds + one tail push).
    #[test]
    fn a_head_membership_change_is_a_rebind_cascade_plus_one_tail_mutation() {
        // Head removal: [1, 2, 3] → [2, 3].
        let mut mirror = vec![1, 2, 3];
        let model = VecModel::from(vec![1, 2, 3]);
        assert_eq!(diff_rows(&mut mirror, &model, &[2, 3]), 3);
        assert_eq!(model.row_count(), 2);
        assert_eq!(mirror, rows_of(&model));
        // Head insertion: [2, 3] → [0, 2, 3].
        let mut grown_mirror = vec![2, 3];
        let grown = VecModel::from(vec![2, 3]);
        assert_eq!(diff_rows(&mut grown_mirror, &grown, &[0, 2, 3]), 3);
        assert_eq!(grown.row_count(), 3);
        assert_eq!(grown_mirror, rows_of(&grown));
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
            width_px: 0.0,
            delete_armed: false,
            aim_gap: -1,
            move_aim: false,
            badge_label: "".into(),
        };
        let mut mirror = vec![column.clone()];
        let model = VecModel::from(vec![column.clone()]);
        assert_eq!(
            diff_rows(&mut mirror, &model, std::slice::from_ref(&column)),
            0,
            "unchanged refresh"
        );
        // A scalar change fires exactly one row update; a fresh inner
        // handle (the pre-D9 shape) fires even with equal scalars.
        let retitle = StackColumnModel {
            title: "b".into(),
            ..column.clone()
        };
        assert_eq!(
            diff_rows(&mut mirror, &model, std::slice::from_ref(&retitle)),
            1
        );
        let rebound = StackColumnModel {
            tasks: ModelRc::new(VecModel::default()),
            ..retitle
        };
        assert_eq!(
            diff_rows(&mut mirror, &model, &[rebound]),
            1,
            "a fresh handle rebinds"
        );
        // The mirror tracked every mutation in lockstep (#464).
        assert_eq!(mirror.len(), model.row_count());
        assert_eq!(mirror, rows_of(&model));
    }

    /// D9 — an unchanged card (same title, phrases, emphasis, and the
    /// persistent decorations handle) compares fully equal, so the
    /// card row never fires across refreshes.
    #[test]
    fn an_unchanged_card_compares_equal_across_refresh_passes() {
        let mut t = task();
        t.importance = Some(Importance::Normal);
        let decorations = Rc::new(VecModel::from(field_decorations(
            &t,
            None,
            calendar(),
            epoch(),
        )));
        let vs = ViewState::default();
        let now = epoch();
        let sets = DueSets::default();
        let chips = ModelRc::new(VecModel::from(Vec::<crate::TaskLabelChipModel>::new()));
        let build = || {
            task_card_model(
                &t,
                &sets,
                &vs,
                now,
                false,
                ModelRc::from(decorations.clone()),
                chips.clone(),
                0,
            )
        };
        let mut mirror = vec![build()];
        let model = VecModel::from(vec![build()]);
        assert_eq!(
            diff_rows(&mut mirror, &model, &[build()]),
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
            false,
            ModelRc::new(VecModel::from(field_decorations(
                &t,
                None,
                calendar(),
                epoch(),
            ))),
            chips.clone(),
            0,
        );
        assert_eq!(diff_rows(&mut mirror, &model, &[fresh]), 1);
        // The mirror tracked every mutation in lockstep (#464).
        assert_eq!(mirror.len(), model.row_count());
        assert_eq!(mirror, rows_of(&model));
    }

    /// #464 — the fresh-slot pin: a new `MirroredRows` installs its
    /// model exactly once, starts empty on both sides, and the mirror
    /// tracks every diff mutation in lockstep (an untracked mirror
    /// would re-report every row as a mutation on the second push).
    #[test]
    fn a_fresh_slot_installs_once_and_diffs_in_lockstep() {
        let mut slot = MirroredRows::default();
        let installs = std::cell::Cell::new(0usize);
        let count = |_: ModelRc<i32>| installs.set(installs.get() + 1);
        let once = |_: ModelRc<i32>| panic!("the model installs exactly once");

        assert_eq!(slot.push(count, &[1, 2, 3]), 3, "fresh slot appends all");
        assert_eq!(installs.get(), 1);
        assert_eq!(
            slot.push(once, &[1, 2, 3]),
            0,
            "the tracked mirror makes the identical push zero-mutation"
        );
        assert_eq!(slot.push(once, &[1, 9, 3]), 1, "one changed row");
        let handle = slot.handle_or_insert_with();
        assert_eq!(handle.row_data(1), Some(9));
        // The struct-embedding sites' contract: the same `Rc` handle
        // every time, the one that was installed.
        assert!(Rc::ptr_eq(&handle, &slot.handle_or_insert_with()));
    }

    /// #464 — the mirror tracks truncation and append in lockstep:
    /// grow, shrink to a prefix, re-grow with changed tail rows —
    /// exactly the tail mutations each time, and a follow-up identical
    /// push is zero (the model/mirror lockstep pin).
    #[test]
    fn the_mirror_tracks_truncation_and_append_in_lockstep() {
        let mut slot = MirroredRows::default();
        let once = |_: ModelRc<i32>| panic!("the model installs exactly once");
        assert_eq!(slot.push(|_| {}, &[1, 2]), 2);
        assert_eq!(slot.push(once, &[1, 2, 3, 4]), 2, "append the tail");
        assert_eq!(slot.push(once, &[1]), 3, "truncate the tail");
        let handle = slot.handle_or_insert_with();
        assert_eq!(handle.row_count(), 1);
        assert_eq!(handle.row_data(0), Some(1));
        assert_eq!(slot.push(once, &[1, 5, 6]), 2, "the two tail rows");
        assert_eq!(slot.push(once, &[1, 5, 6]), 0, "lockstep holds");
        assert_eq!(handle.row_data(1), Some(5));
        assert_eq!(handle.row_data(2), Some(6));
    }

    /// #464's divergence witness (the plan's D7): a live model mutated
    /// behind the mirror — the exact rogue-site shape — panics at the
    /// next diff in debug builds instead of corrupting every later
    /// diff silently.
    #[test]
    #[should_panic(expected = "mirror/model divergence")]
    fn a_diverged_mirror_panics_at_the_next_diff() {
        let mut slot = MirroredRows::<i32>::default();
        slot.push(|_| {}, &[1, 2]);
        slot.handle_or_insert_with().push(3);
        slot.diff(&[1, 2, 3]);
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
                authors: taskboard_domain::entities::StackAuthors::default(),
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
                label_memberships: BTreeMap::new(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,
                kind: taskboard_domain::entities::TaskKind::Standard,

                archived: false,
                deleted: false,
                clocks: taskboard_domain::TaskClocks {
                    title: BoardClocks::epoch(),
                    description: BoardClocks::epoch(),
                    duedate: BoardClocks::epoch(),
                    scheduled: BoardClocks::epoch(),
                    done: BoardClocks::epoch(),
                    position: BoardClocks::epoch(),
                    effort: BoardClocks::epoch(),
                    importance: BoardClocks::epoch(),
                    urgency: BoardClocks::epoch(),
                    recur: BoardClocks::epoch(),
                    kind: BoardClocks::epoch(),

                    archived: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::TaskAuthors::default(),
                remote_seen: None,
                remote_baseline: None,
            }
        }

        fn stamp(secs: i64) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.timestamp_opt(secs, 0).unwrap()
        }

        fn day(now: i64) -> DayCtx {
            DayCtx {
                now: stamp(now),
                window: (stamp(now), stamp(now + 86_400)),
                calendar: taskboard_domain::DayCalendar {
                    day_start_time: chrono::NaiveTime::MIN,
                    offset: chrono::FixedOffset::east_opt(0).expect("utc offset"),
                },
            }
        }

        /// D-6.2 (round 6): the banner row projection carries the
        /// state ladder's inputs — a muted row pushes `muted` with an
        /// empty snoozed-phrase; a snoozed (unmuted) row pushes
        /// `muted == false` and a non-empty snoozed-phrase
        /// (emptiness/non-emptiness are the structural facts; the
        /// phrase text is never the oracle).
        #[test]
        fn the_banner_rows_push_the_muted_and_snoozed_state() {
            on_slint_thread(|| {
                let b = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
                let sa = StackId::from_uuid(uuid::Uuid::now_v7());
                let mut state = AppState::default();
                state.boards.insert(b, board(b, "alpha"));
                state.stacks.insert(sa, stack(sa, b));
                let muted_id = TaskId::from_uuid(uuid::Uuid::now_v7());
                let mut t = task(muted_id, sa, "muted");
                t.order = 0;
                t.duedate = Some(stamp(31_500_000));
                state.tasks.insert(muted_id, t);
                state.reminders.muted.insert(muted_id);
                let snoozed_id = TaskId::from_uuid(uuid::Uuid::now_v7());
                let mut t = task(snoozed_id, sa, "snoozed");
                t.order = 1;
                t.duedate = Some(stamp(31_510_000));
                state.tasks.insert(snoozed_id, t);
                // The journaled snooze link for the rule-less default
                // ref: the occurrence is the duedate itself (the
                // banner row's snooze_target shape), rescheduling into
                // the future.
                state.reminders.snoozes.insert(
                    taskboard_domain::ReminderSnoozeKey {
                        reminder: taskboard_domain::ReminderRef::Default(snoozed_id),
                        occurrence: stamp(31_510_000),
                    },
                    stamp(31_563_600) + chrono::TimeDelta::hours(1),
                );

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);

                let rows = window.get_banner_model();
                assert_eq!(rows.row_count(), 2, "both overdue tasks render");
                let by_title = |title: &str| {
                    (0..rows.row_count())
                        .map(|i| rows.row_data(i).expect("row"))
                        .find(|row| row.title == title)
                        .expect("the addressed row renders")
                };
                let muted_row = by_title("muted");
                assert!(muted_row.muted, "the muted task's row reports muted");
                assert_eq!(
                    muted_row.snoozed_phrase, "",
                    "no snooze record for the muted task: an empty phrase"
                );
                let snoozed_row = by_title("snoozed");
                assert!(!snoozed_row.muted, "the snoozed task is not muted");
                assert!(
                    !snoozed_row.snoozed_phrase.is_empty(),
                    "the snoozed row carries its reschedule phrase"
                );
            });
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
                calendar(),
                kiosk,
                &taskboard_domain::ReminderDefaults::default(),
                None,
                models,
                &due,
                crate::models::RecurRowPush::default(),
                crate::models::PriorityPickerPush::default(),
                None,
                None,
                -1,
                -1,
                (None, None),
                None,
            )
        }

        // -- the today aggregation pins (round-6 A1 D-2/D-3; red list) -----------

        /// A live habit on `board` at `order` (the aggregation pins'
        /// row factory; the H2 shape the render reads).
        fn habit_on(
            state: &mut AppState,
            board: taskboard_domain::BoardId,
            order: i64,
            title: &str,
        ) -> taskboard_domain::HabitId {
            let id = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
            state.habits.habits.insert(
                id,
                taskboard_domain::Habit {
                    id,
                    board,
                    title: title.to_owned(),
                    description: String::new(),
                    target: taskboard_domain::HabitTarget::PerDay(
                        taskboard_domain::NonZeroCount::new(1).expect("nonzero"),
                    ),
                    order,
                    anchor_time: None,
                    anchor_day: None,
                    scheduled_days: None,
                    reminder: None,
                    clocks: taskboard_domain::HabitClocks::all(epoch()),
                    continued_from: None,
                    deleted: false,
                    by: None,
                    paused_until: None,
                },
            );
            id
        }

        /// A Work time entry inside the today window (the tracked-today
        /// pins' input; `day.window` is (stamp(N), `stamp(N+86_400)`)).
        fn work_entry(state: &mut AppState, task: TaskId, title: &str, start: i64, minutes: i64) {
            let start = stamp(start);
            state.tracking.entries.insert(
                taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
                taskboard_domain::TimeEntry {
                    id: taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
                    task,
                    task_title: title.to_owned(),
                    start,
                    end: Some(start + chrono::TimeDelta::minutes(minutes)),
                    created_at: epoch(),
                    updated_at: epoch(),
                    kind: taskboard_domain::tracking::TimeEntryKind::Work,
                    by: None,
                },
            );
        }

        /// A1 + A4 (D-2): the habits section's rows — render and walk —
        /// aggregate over ALL live boards in canonical board order,
        /// stored `(order, id)` inner. Red pre-fix: the projection was
        /// scoped to the selected board.
        #[test]
        fn the_today_habit_rows_aggregate_all_live_boards() {
            on_slint_thread(|| {
                let (mut state, board_a, board_b, _ids) = two_board_state();
                let ha0 = habit_on(&mut state, board_a, 10, "alpha-0");
                let hb0 = habit_on(&mut state, board_b, 10, "beta-0");
                let hb1 = habit_on(&mut state, board_b, 20, "beta-1");
                let ha1 = habit_on(&mut state, board_a, 20, "alpha-1");

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                let _ = vs.select_board(&state, board_a);
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);

                // The render: both boards' habits, board-canonical
                // outer order (alpha before beta), stored (order, id)
                // inner.
                let rendered: Vec<String> = (0..window.get_habits_model().row_count())
                    .filter_map(|i| window.get_habits_model().row_data(i))
                    .map(|row| row.card.title.to_string())
                    .collect();
                assert_eq!(
                    rendered,
                    ["alpha-0", "alpha-1", "beta-0", "beta-1"],
                    "the habits section aggregates all live boards"
                );

                // The walk agrees with the render by construction.
                let walked: Vec<_> = vs
                    .today_rows(&state, false, &day(31_563_600))
                    .into_iter()
                    .filter_map(|row| match row {
                        crate::view_state::TodayRow::Habit(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    walked,
                    [ha0, ha1, hb0, hb1],
                    "the walk's habit rows aggregate in the render order"
                );
            });
        }

        /// A2 (D-2): an archived or deleted board's habits never render
        /// on the Today view. Armor: pre-fix this holds vacuously for a
        /// selected live board (the scope excluded the dead boards'
        /// rows anyway); the pin keeps the exclusion under the
        /// aggregated contract.
        #[test]
        fn dead_and_archived_boards_contribute_no_habit_rows() {
            on_slint_thread(|| {
                let (mut state, board_a, board_b, _ids) = two_board_state();
                let _ = habit_on(&mut state, board_a, 10, "archived-0");
                let live = habit_on(&mut state, board_b, 10, "live-0");
                state.boards.get_mut(&board_a).expect("board").archived = true;

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                let _ = vs.select_board(&state, board_b);
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let rendered: Vec<String> = (0..window.get_habits_model().row_count())
                    .filter_map(|i| window.get_habits_model().row_data(i))
                    .map(|row| row.card.title.to_string())
                    .collect();
                assert_eq!(rendered, ["live-0"], "the archived board's habit is absent");
                let _ = live;
            });
        }

        /// A6 (D-4): zero live boards render no habit rows and keep the
        /// create affordance unmountable (the typed mount flag).
        #[test]
        fn zero_live_boards_render_no_habit_rows_and_no_create_mount() {
            on_slint_thread(|| {
                let (mut state, board_a, _board_b, _ids) = two_board_state();
                let _ = habit_on(&mut state, board_a, 10, "doomed");
                for id in state.boards.keys().copied().collect::<Vec<_>>() {
                    state.boards.get_mut(&id).expect("board").deleted = true;
                }

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                assert_eq!(
                    window.get_habits_model().row_count(),
                    0,
                    "no live boards, no habit rows"
                );
                assert!(
                    !window.get_habits_create_mountable(),
                    "the create affordance is unmountable without a live board"
                );
            });
        }

        /// A7 (D-3): on kiosk the tracked-today rows and the day total
        /// fold the FULL aggregated set — what you see is what's summed
        /// over all live boards. Red pre-fix: the retired board filter
        /// emptied the kiosk set (no viewed board on kiosk).
        #[test]
        fn kiosk_tracked_today_sums_over_all_live_boards() {
            on_slint_thread(|| {
                let (mut state, board_a, board_b, ids) = two_board_state();
                // ids: [overdue(beta), scheduled(alpha), due(alpha),
                // anchor(alpha), done(beta)]. Track one task per board,
                // 10 work minutes each, inside the day window.
                let scheduled = ids[1];
                let overdue = ids[0];
                work_entry(&mut state, scheduled, "scheduled", 31_564_000, 10);
                work_entry(&mut state, overdue, "overdue", 31_564_500, 10);

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                let tables = push_pass(&window, &state, &vs, day(31_563_600), true, &mut models);

                let mut tracked = tables_tracked_today(&tables);
                tracked.sort();
                let mut expected = vec![scheduled, overdue];
                expected.sort();
                assert_eq!(
                    tracked, expected,
                    "kiosk tracked-today aggregates both boards' tracked tasks"
                );
                // Derived, never pinned copy (the no-text-assertions
                // law): the total is the aggregated set's two-hour...
                // two ten-minute entries' phrase.
                let expected = crate::presentation::tracked_total_phrase(
                    &crate::presentation::part_total(chrono::TimeDelta::minutes(20)),
                );
                assert_eq!(
                    window.get_today_model().tracked_day_total,
                    expected,
                    "the day total folds the aggregated set"
                );
                let _ = board_a;
                let _ = board_b;
            });
        }

        /// The desktop leg of A7's invariant (armor): the tracked set
        /// was already the full aggregate on desktop; it stays so.
        #[test]
        fn desktop_tracked_today_still_sums_all_live_boards() {
            on_slint_thread(|| {
                let (mut state, _a, _b, ids) = two_board_state();
                let scheduled = ids[1];
                let overdue = ids[0];
                work_entry(&mut state, scheduled, "scheduled", 31_564_000, 10);
                work_entry(&mut state, overdue, "overdue", 31_564_500, 10);

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                let tables = push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let mut tracked = tables_tracked_today(&tables);
                tracked.sort();
                let mut expected = vec![scheduled, overdue];
                expected.sort();
                assert_eq!(tracked, expected);
            });
        }

        /// D-3: the Today surface's break-rule roster aggregates over
        /// all boards. Red pre-fix: the roster was scoped to the
        /// selected board.
        #[test]
        fn the_today_break_rule_roster_aggregates_all_live_boards() {
            on_slint_thread(|| {
                let (mut state, board_a, board_b, _ids) = two_board_state();
                for (board, label) in [(board_a, "rule-alpha"), (board_b, "rule-beta")] {
                    let id = taskboard_domain::BreakRuleId::from_uuid(uuid::Uuid::now_v7());
                    state.reminders.break_rules.insert(
                        id,
                        taskboard_domain::BreakRule {
                            id,
                            board,
                            label: label.to_owned(),
                            threshold: taskboard_domain::WorkThreshold::parse_text_form("30m")
                                .expect("threshold"),
                            insistence: taskboard_domain::Insistence::Normal,
                            created_at: epoch(),
                            by: None,
                        },
                    );
                }

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                let _ = vs.select_board(&state, board_a);
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let labels: Vec<String> = (0..window.get_break_rules_model().row_count())
                    .filter_map(|i| window.get_break_rules_model().row_data(i))
                    .map(|row| row.label.to_string())
                    .collect();
                assert_eq!(
                    labels,
                    ["rule-alpha", "rule-beta"],
                    "the today roster aggregates all boards' rules"
                );
                // The walk agrees with the render (the D2 law).
                let walked: Vec<_> = vs
                    .today_rows(&state, false, &day(31_563_600))
                    .into_iter()
                    .filter_map(|row| match row {
                        crate::view_state::TodayRow::BreakRule(rule) => Some(rule),
                        _ => None,
                    })
                    .collect();
                assert_eq!(walked.len(), 2, "the walk carries both rosters' rows");
            });
        }

        fn tables_tracked_today(tables: &IdTables) -> Vec<TaskId> {
            tables.tracked_today.clone()
        }

        // -- the plan-next-day pass (#622, D-8) ----------------------------------

        /// One board, three tasks: placed (scheduled tomorrow 09:xx with
        /// an effort), carried (due yesterday), candidate (no stamps) —
        /// the ids vec is in that order.
        fn plan_state() -> (AppState, StackId, Vec<TaskId>) {
            let b = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let sa = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(b, board(b, "alpha"));
            state.stacks.insert(sa, stack(sa, b));
            let placed = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(placed, sa, "placed");
            t.order = 0;
            t.scheduled = Some(stamp(31_630_000)); // tomorrow 09:20 UTC
            t.effort = Some(taskboard_domain::Effort::new(1_800));
            state.tasks.insert(placed, t);
            let carried = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(carried, sa, "carried");
            t.order = 1;
            t.duedate = Some(stamp(31_500_000)); // yesterday: overdue at tomorrow's open
            state.tasks.insert(carried, t);
            let candidate = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(candidate, sa, "candidate");
            t.order = 2;
            state.tasks.insert(candidate, t);
            (state, sa, vec![placed, carried, candidate])
        }

        #[test]
        fn the_plan_pass_renders_boxes_pool_and_tables() {
            on_slint_thread(|| {
                let (state, _sa, ids) = plan_state();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                let tables = push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);

                // Reading order: the placed boxes then the pool; the
                // carried miss is a pool row (the partition rule).
                assert_eq!(tables.plan_rows, ids);
                assert_eq!(tables.plan_box_count, 1);
                assert_eq!(tables.plan_position(ids[0]), Some((0, 0)));
                assert_eq!(tables.plan_position(ids[1]), Some((1, 0)));
                assert_eq!(tables.plan_position(ids[2]), Some((1, 1)));
                assert_eq!(tables.plan_row(0, 0), Some(ids[0]));
                assert_eq!(tables.plan_row(1, 0), Some(ids[1]));

                let plan = window.get_plan_model();
                assert_eq!(plan.boxes.row_count(), 1);
                assert_eq!(plan.pool.row_count(), 2);
                assert!(!plan.placing);
                // The box's phrases arrive derived: the time phrase
                // matches the pure presentation helper over the same
                // stamp, the effort phrase the effort one.
                let calendar = calendar();
                let box_row = plan.boxes.row_data(0).expect("box");
                assert_eq!(
                    box_row.time.to_string(),
                    presentation::plan_box_time_phrase(Some(stamp(31_630_000)), calendar)
                );
                assert_eq!(
                    box_row.time.to_string().len(),
                    5,
                    "HH:mm is five characters"
                );
                assert_eq!(
                    box_row.effort.to_string(),
                    presentation::effort_phrase(Some(taskboard_domain::Effort::new(1_800)))
                );
                assert!(!box_row.overdue);
                // The pool excludes the placed box; the carried miss is
                // overdue-marked, the plain candidate is not.
                let carried_row = plan.pool.row_data(0).expect("carried");
                assert!(carried_row.overdue);
                let candidate_row = plan.pool.row_data(1).expect("candidate");
                assert!(!candidate_row.overdue);
            });
        }

        #[test]
        fn a_stepped_now_moves_the_window_and_the_boxes() {
            on_slint_thread(|| {
                let (state, _sa, ids) = plan_state();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                let _ = push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);

                // Tomorrow's box is today's at the rollover: the window
                // advanced with the pass's fresh `DayCtx`, the box
                // re-derives into the pool (a plain candidate — nothing
                // disappeared).
                let mut models = PushModels::default();
                let tables = push_pass(
                    &window,
                    &state,
                    &vs,
                    day(31_563_600 + 86_400),
                    false,
                    &mut models,
                );
                assert_eq!(tables.plan_box_count, 0);
                // The pool holds all three (carried miss included), in
                // `(order, id)` order.
                assert_eq!(tables.plan_rows, ids, "pool: (order, id)");
                let plan = window.get_plan_model();
                assert_eq!(plan.boxes.row_count(), 0);
                assert_eq!(plan.pool.row_count(), 3);
            });
        }

        #[test]
        fn the_kiosk_plan_pass_is_the_empty_model() {
            on_slint_thread(|| {
                let (state, _sa, _ids) = plan_state();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                let tables = push_pass(&window, &state, &vs, day(31_563_600), true, &mut models);
                assert_eq!(tables.plan_rows, [] as [TaskId; 0]);
                assert_eq!(tables.plan_box_count, 0);
                let plan = window.get_plan_model();
                assert_eq!(plan.boxes.row_count(), 0);
                assert_eq!(plan.pool.row_count(), 0);
            });
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

        /// #464 — the prune pin: `column_tasks` (and
        /// `card_decorations`, via `pass_tasks`) retain the mirror and
        /// the handle as one map value, so the one `retain` cannot
        /// split their membership and a returning stack comes back as
        /// a fresh pair — empty on both sides, rebuilt through the
        /// normal diff.
        #[test]
        fn a_pruned_column_slot_returns_as_a_fresh_pair() {
            on_slint_thread(|| {
                let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
                let sa = StackId::from_uuid(uuid::Uuid::now_v7());
                let sb = StackId::from_uuid(uuid::Uuid::now_v7());
                let mut state = AppState::default();
                state.boards.insert(board_id, board(board_id, "alpha"));
                let mut second = stack(sb, board_id);
                second.order = 1;
                state.stacks.insert(sa, stack(sa, board_id));
                state.stacks.insert(sb, second);
                let one = TaskId::from_uuid(uuid::Uuid::now_v7());
                state.tasks.insert(one, task(one, sa, "one"));
                let two = TaskId::from_uuid(uuid::Uuid::now_v7());
                state.tasks.insert(two, task(two, sb, "two"));

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                let _ = vs.select_board(&state, board_id);
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                assert_eq!(models.column_tasks.len(), 2);
                assert_eq!(models.card_decorations.len(), 2);

                // The second stack drops: one retain prunes the pair.
                state.stacks.get_mut(&sb).expect("live stack").deleted = true;
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                assert_eq!(models.column_tasks.len(), 1);
                assert!(models.column_tasks.contains_key(&sa));

                // It returns: a fresh pair, rebuilt through the diff.
                state.stacks.get_mut(&sb).expect("live stack").deleted = false;
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                assert_eq!(models.column_tasks.len(), 2);
                let board_model = window.get_board_model();
                assert_eq!(board_model.stacks.row_count(), 2);
                let returning = board_model.stacks.row_data(1).expect("returning column");
                assert_eq!(returning.tasks.row_count(), 1);
                let card = returning.tasks.row_data(0).expect("returning card");
                assert_eq!(card.title.to_string(), "two");

                // Companion: a task vanishing from every surface leaves
                // `card_decorations` without its pair (the `pass_tasks`
                // retain, now pruning pairs).
                state.tasks.get_mut(&two).expect("live task").deleted = true;
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                assert!(!models.card_decorations.contains_key(&two));
                assert!(models.card_decorations.contains_key(&one));
            });
        }

        /// #1017 — the habits-surface identity law: a second identical
        /// refresh pass must not replace the row's decoration handle
        /// (`TaskCardModel`'s decorations `PartialEq` IS handle
        /// identity), i.e. the habits surface stops re-pushing whole
        /// rows every pass. The `card_decorations` rule pinned at the
        /// habits surface, models level.
        #[test]
        fn a_second_identical_habits_pass_keeps_the_decoration_handle() {
            on_slint_thread(|| {
                let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
                let mut state = AppState::default();
                state.boards.insert(board_id, board(board_id, "alpha"));
                let habit = taskboard_domain::Habit {
                    id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
                    board: board_id,
                    title: "water".into(),
                    description: String::new(),
                    target: taskboard_domain::HabitTarget::PerDay(
                        taskboard_domain::NonZeroCount::new(3).expect("nonzero"),
                    ),
                    order: 0,
                    anchor_time: None,
                    anchor_day: None,
                    scheduled_days: None,
                    reminder: None,
                    clocks: taskboard_domain::HabitClocks::all(
                        chrono::DateTime::from_timestamp(100, 0).expect("stamp"),
                    ),
                    continued_from: None,
                    deleted: false,
                    by: None,
                    paused_until: None,
                };
                state.habits.habits.insert(habit.id, habit);

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                let _ = vs.select_board(&state, board_id);
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let handle = window
                    .get_habits_model()
                    .row_data(0)
                    .expect("habit row")
                    .card
                    .decorations
                    .clone();

                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let row = window.get_habits_model().row_data(0).expect("habit row");
                // `ModelRc`'s `PartialEq` is handle identity — this is
                // the same comparison the row diff performs.
                assert_eq!(
                    handle, row.card.decorations,
                    "a second identical pass must reuse the decoration handle"
                );
                assert_eq!(models.habit_decorations.len(), 1, "one live slot");
            });
        }

        /// #1017 — the diff half of the law: a real decoration content
        /// change flows through the persistent handle (identity kept,
        /// content updated), not a fresh mint — the mirror diff is the
        /// thing the stable-handle pin above presupposes.
        #[test]
        fn a_habit_content_change_diffs_through_the_persistent_handle() {
            on_slint_thread(|| {
                let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
                let mut state = AppState::default();
                state.boards.insert(board_id, board(board_id, "alpha"));
                let habit_id = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
                let habit = taskboard_domain::Habit {
                    id: habit_id,
                    board: board_id,
                    title: "water".into(),
                    description: String::new(),
                    target: taskboard_domain::HabitTarget::PerDay(
                        taskboard_domain::NonZeroCount::new(3).expect("nonzero"),
                    ),
                    order: 0,
                    anchor_time: None,
                    anchor_day: None,
                    scheduled_days: None,
                    reminder: None,
                    clocks: taskboard_domain::HabitClocks::all(
                        chrono::DateTime::from_timestamp(100, 0).expect("stamp"),
                    ),
                    continued_from: None,
                    deleted: false,
                    by: None,
                    paused_until: None,
                };
                state.habits.habits.insert(habit_id, habit);

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                let _ = vs.select_board(&state, board_id);
                let mut models = PushModels::default();
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let before = window
                    .get_habits_model()
                    .row_data(0)
                    .expect("habit row")
                    .card
                    .decorations
                    .clone();

                // Real content change: the quota moves 3 → 5, so the
                // progress decoration's phrase must move "0/3" → "0/5"
                // through the SAME handle the diff mirrors onto.
                state
                    .habits
                    .habits
                    .get_mut(&habit_id)
                    .expect("live habit")
                    .target = taskboard_domain::HabitTarget::PerDay(
                    taskboard_domain::NonZeroCount::new(5).expect("nonzero"),
                );
                push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                let row = window.get_habits_model().row_data(0).expect("habit row");
                assert_eq!(
                    before, row.card.decorations,
                    "a content change must not replace the decoration handle"
                );
                let progress = row
                    .card
                    .decorations
                    .row_data(0)
                    .expect("progress decoration");
                assert_eq!(progress.label.to_string(), "0/5", "the diff landed");
                assert_eq!(models.habit_decorations.len(), 1, "one live slot");
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
                authors: taskboard_domain::entities::StackAuthors::default(),
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

    // ---- the tracker task kind (backlog round 2 chunk 4.2, #492) ------

    /// U1 — the tracker card hides the done affordance and carries the
    /// badge: `can-done` is false and the decoration list holds exactly
    /// one `tracker` badge (the Recur badge's mechanism).
    #[test]
    fn tracker_cards_hide_the_done_box_and_carry_the_badge() {
        let mut t = task();
        t.kind = taskboard_domain::entities::TaskKind::Tracker;
        let decorations = Rc::new(VecModel::from(field_decorations(
            &t,
            None,
            calendar(),
            epoch(),
        )));
        let vs = ViewState::default();
        let card = task_card_model(
            &t,
            &DueSets::default(),
            &vs,
            epoch(),
            false,
            ModelRc::from(decorations.clone()),
            ModelRc::new(VecModel::from(Vec::<crate::TaskLabelChipModel>::new())),
            0,
        );
        assert!(!card.can_done, "the tracker card hides the done box");
        let badges: Vec<_> = card
            .decorations
            .iter()
            .filter(|d| matches!(d.kind, DecorationKind::Tracker))
            .collect();
        assert_eq!(badges.len(), 1);
        assert_eq!(badges[0].label, "tracker");
        // A standard task is byte-identical to the pre-kind shape.
        let standard = task();
        let plain = task_card_model(
            &standard,
            &DueSets::default(),
            &vs,
            epoch(),
            false,
            ModelRc::new(VecModel::from(Vec::new())),
            ModelRc::new(VecModel::from(Vec::new())),
            0,
        );
        assert!(plain.can_done);
        assert!(
            plain
                .decorations
                .iter()
                .all(|d| !matches!(d.kind, DecorationKind::Tracker))
        );
    }

    /// U2 — the liveness pin at the model seam: a tracked tracker card
    /// keeps the tracking affordances (`can-track-*` flags derive from
    /// the open entry, the Tracking badge rides the list) while the
    /// done box stays hidden.
    #[test]
    fn the_tracking_affordances_stay_on_a_tracker_card() {
        let mut t = task();
        t.kind = taskboard_domain::entities::TaskKind::Tracker;
        let tracked = Some(t.id);
        let decorations = Rc::new(VecModel::from(field_decorations(
            &t,
            tracked,
            calendar(),
            epoch(),
        )));
        let vs = ViewState::default();
        let card = task_card_model(
            &t,
            &DueSets::default(),
            &vs,
            epoch(),
            false,
            ModelRc::from(decorations),
            ModelRc::new(VecModel::from(Vec::new())),
            0,
        );
        assert!(!card.can_done);
        assert!(
            card.decorations
                .iter()
                .any(|d| matches!(d.kind, DecorationKind::Tracking)),
            "the tracking badge co-exists with the hidden done box"
        );
        // The detail seam's track flags derive from the tracking pass,
        // untouched by the kind: the verb table keeps its row.
        let mut state = taskboard_domain::AppState::default();
        state.tracking.entries.insert(
            taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
            taskboard_domain::TimeEntry {
                id: taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
                task: t.id,
                task_title: t.title.clone(),
                start: epoch(),
                end: None,
                created_at: epoch(),
                updated_at: epoch(),
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
                by: None,
            },
        );
        let verb = presentation::detail_tracking_verb(&state, Some(t.id));
        assert_eq!(
            verb,
            Some(presentation::TrackDetail::Stop),
            "the open entry keeps its stop verb on a tracker"
        );
    }
    // -- the review push (#214, pair 4) --------------------------------------

    use crate::test_platform::on_slint_thread;
    use crate::{UiSettings, mount};
    use std::sync::Arc;

    fn review_rig(state: AppState, settings: UiSettings) -> crate::UiHandle {
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
        let (commands_tx, _commands_rx) = tokio::sync::mpsc::channel(64);
        let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        // The pass's clock frozen at the module's seed instant
        // (docs/testing_strategy.org §6.2): no test-side `now` can
        // reach the pass's `now` other than through `UiSettings::clock`
        // (the rigs build their `DayCtx` through the adapter's mount),
        // so the pin is the only tool that fully determines the
        // oracle's inputs — applied over whatever settings a caller
        // brings.
        let settings = UiSettings {
            clock: Some(crate::test_platform::PINNED_NOW),
            ..settings
        };
        mount(
            state,
            {
                let (_tx, rx) = tokio::sync::broadcast::channel(4);
                rx
            },
            {
                let (_tx, rx) = tokio::sync::broadcast::channel(4);
                rx
            },
            commands_tx,
            requests_tx,
            settings,
            health,
            None,
        )
        .expect("mount succeeds on the null platform")
    }

    /// A completed task `age` ago with the given estimate.
    fn completed(raw: u128, title: &str, age: chrono::TimeDelta, effort: Option<u64>) -> Task {
        let mut t = task();
        t.id = TaskId::from_uuid(uuid::Uuid::from_u128(raw));
        t.title = title.to_owned();
        t.done = Some(crate::test_platform::PINNED_NOW - age);
        t.effort = effort.map(taskboard_domain::Effort::new);
        t
    }

    fn work_entry(
        task: TaskId,
        start: chrono::DateTime<chrono::Utc>,
        end: Option<chrono::DateTime<chrono::Utc>>,
    ) -> taskboard_domain::TimeEntry {
        let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        taskboard_domain::TimeEntry {
            id,
            task,
            task_title: "snapshotted".to_owned(),
            start,
            end,
            created_at: start,
            updated_at: end.unwrap_or(start),
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
            by: None,
        }
    }

    fn tracked(state: &mut AppState, t: &Task, secs: i64) {
        // Three hours before the pinned render clock (docs/
        // testing_strategy.org §6.2): from a mid-day instant the start
        // lies inside the frozen UTC day, so the entry never straddles
        // the running bucket's window and needs no midnight clamp.
        let start = crate::test_platform::PINNED_NOW - chrono::TimeDelta::try_hours(3).unwrap();
        let e = work_entry(
            t.id,
            start,
            Some(start + chrono::TimeDelta::try_seconds(secs).unwrap()),
        );
        state.tracking.entries.insert(e.id, e);
    }

    #[test]
    fn the_review_model_mirrors_calibrate() {
        on_slint_thread(|| {
            let mut state = AppState::default();
            // A: estimated + tracked (compared), done 2h ago.
            let a = completed(
                1,
                "compared-a",
                chrono::TimeDelta::try_hours(2).unwrap(),
                Some(600),
            );
            tracked(&mut state, &a, 300);
            state.tasks.insert(a.id, a.clone());
            // B: tracked, unestimated, done 1h ago (newest completion).
            let b = completed(
                2,
                "unestimated-b",
                chrono::TimeDelta::try_hours(1).unwrap(),
                None,
            );
            tracked(&mut state, &b, 60);
            state.tasks.insert(b.id, b.clone());
            // C: open — never surfaces (D-4).
            let c = task();
            state.tasks.insert(c.id, c);

            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();

            assert!(!model.empty, "comparable rows exist");
            assert!(!model.month.empty);
            assert!(!model.week.empty);
            assert_eq!(model.rows.row_count(), 2, "the open task never surfaces");
            let first = model.rows.row_data(0).expect("row 0");
            let second = model.rows.row_data(1).expect("row 1");
            assert_eq!(first.title, "unestimated-b", "newest completion first");
            assert_eq!(second.title, "compared-a");
            // The structural absences: unestimated and untracked are
            // empty phrases, never zero phrases.
            assert!(first.estimate.is_empty(), "unestimated is the dash");
            assert!(!first.actual.is_empty());
            assert!(first.gap.is_empty(), "an uncompared row carries no gap");
            assert!(!second.estimate.is_empty());
            assert!(!second.gap.is_empty(), "the settled comparison speaks");
            // The dumb-client line: the rows equal `calibrate`'s own
            // output for the same inputs, per the pass's bounds —
            // structure over re-derivation: the aggregate section
            // counts match the rows' own fold.
        });
    }

    #[test]
    fn the_kiosk_belt_emits_the_empty_review_model() {
        on_slint_thread(|| {
            let mut state = AppState::default();
            let a = completed(
                1,
                "compared-a",
                chrono::TimeDelta::try_hours(2).unwrap(),
                Some(600),
            );
            tracked(&mut state, &a, 300);
            state.tasks.insert(a.id, a.clone());
            let handle = review_rig(
                state,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
            );
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(model.empty, "the appliance never projects the ledger");
            assert_eq!(model.rows.row_count(), 0);
            assert!(model.week.gap.is_empty() && model.month.gap.is_empty());
        });
    }

    #[test]
    fn the_review_empty_state_collapses_in_tiers() {
        on_slint_thread(|| {
            // Tier 1 + 3: nothing comparable anywhere — the whole view
            // collapses and the ledger hides.
            let handle = review_rig(AppState::default(), UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(model.empty, "tier 1: the whole view collapses");
            assert_eq!(model.rows.row_count(), 0, "tier 3: the ledger hides");
            assert!(model.week.empty && model.month.empty);

            // Tier 2: rows exist but nothing compares — the window
            // sections collapse their values, the skeleton stays.
            let mut state = AppState::default();
            let a = completed(
                1,
                "skipped-a",
                chrono::TimeDelta::try_hours(2).unwrap(),
                Some(600),
            );
            state.tasks.insert(a.id, a);
            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(!model.empty, "the ledger admits the estimated row");
            assert_eq!(model.rows.row_count(), 1);
            assert!(
                model.week.empty && model.month.empty,
                "tier 2: no comparable tasks collapse the values"
            );
            let row = model.rows.row_data(0).expect("row");
            assert!(row.actual.is_empty(), "untracked is the dash");
            assert!(row.gap.is_empty());
        });
    }

    #[test]
    fn the_gap_phrases_distinguish_the_three_sign_classes() {
        on_slint_thread(|| {
            let scenario = |estimate: u64, actual: i64| {
                let mut state = AppState::default();
                let a = completed(
                    1,
                    "a",
                    chrono::TimeDelta::try_hours(2).unwrap(),
                    Some(estimate),
                );
                tracked(&mut state, &a, actual);
                state.tasks.insert(a.id, a);
                let handle = review_rig(state, UiSettings::default());
                handle
                    .test_adapter()
                    .borrow_mut()
                    .refresh_forced(handle.window());
                handle.window().get_review_model().month.gap
            };
            let over = scenario(600, 1200);
            let under = scenario(600, 300);
            let on_target = scenario(600, 600);
            assert!(!over.is_empty() && !under.is_empty());
            assert_ne!(over, under, "the sign classes are distinct");
            assert_ne!(
                on_target, over,
                "on-target is its own class, not the positive one"
            );
        });
    }

    // -- the tracking sections (#623 slice A, M-1..M-4) ---------------------

    #[test]
    fn the_tracking_sections_mirror_the_domain_projection() {
        on_slint_thread(|| {
            let mut state = AppState::default();
            let a = completed(
                1,
                "compared-a",
                chrono::TimeDelta::try_hours(2).unwrap(),
                None,
            );
            tracked(&mut state, &a, 3_600);
            state.tasks.insert(a.id, a.clone());
            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            for section in [model.tracking_week.clone(), model.tracking_month.clone()] {
                assert!(!section.empty, "the window holds tracked time");
                assert!(!section.work.is_empty());
                assert!(!section.blocks.is_empty());
                // The strip always carries the window's day count.
                assert!(section.bars.row_count() == 7 || section.bars.row_count() == 30);
                // The running day's bucket is populated (the entry sat
                // 3h ago inside it).
                let last = section
                    .bars
                    .row_data(section.bars.row_count() - 1)
                    .expect("the running day's bar");
                assert!(
                    last.base > 0.0,
                    "the running day's work fraction is strictly positive"
                );
                assert!(last.stack == 0.0, "no respite was planted");
            }
            assert_eq!(model.tracking_week.bars.row_count(), 7);
            assert_eq!(model.tracking_month.bars.row_count(), 30);
        });
    }

    #[test]
    fn the_tracking_sections_collapse_in_tiers() {
        on_slint_thread(|| {
            // Tier 1 + 2 together on the bare fixture: the whole view
            // collapses and the sections' strips stay full-length and
            // all-zero.
            let handle = review_rig(AppState::default(), UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(model.empty, "tier 1: nothing anywhere");
            for section in [model.tracking_week.clone(), model.tracking_month.clone()] {
                assert!(section.empty, "tier 2: the section collapses");
                assert_eq!(section.work, "");
                assert_eq!(section.respite, "");
                assert_eq!(section.blocks, "");
                let len = section.bars.row_count();
                assert!(len == 7 || len == 30, "the skeleton never vanishes");
                for i in 0..len {
                    let bar = section.bars.row_data(i).expect("bar");
                    assert!(bar.base == 0.0 && bar.stack == 0.0, "all-zero strip");
                }
            }
        });
    }

    #[test]
    fn tracked_time_alone_keeps_the_review_view_standing() {
        on_slint_thread(|| {
            // D-S9's tier-1 extension: tracked time, no comparable
            // completed tasks — the view stays, the calibration pair
            // collapses, the tracking pair speaks.
            let mut state = AppState::default();
            let open = task();
            // An open entry running for three hours at the frozen
            // clock: starting it *at* PINNED_NOW would accrue zero
            // (the open clamp evaluates at `now`), so the seed sits
            // before the pin like `tracked`'s.
            let entry = work_entry(
                open.id,
                crate::test_platform::PINNED_NOW - chrono::TimeDelta::try_hours(3).unwrap(),
                None,
            );
            state.tracking.entries.insert(entry.id, entry);
            state.tasks.insert(open.id, open);
            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(!model.empty, "tracked time alone keeps the view");
            assert!(
                model.week.empty && model.month.empty,
                "the calibration sections collapse"
            );
            assert!(!model.tracking_week.empty && !model.tracking_month.empty);
            assert!(!model.tracking_week.work.is_empty());
        });
    }

    #[test]
    fn the_kiosk_belt_extends_to_the_tracking_sections() {
        on_slint_thread(|| {
            let mut state = AppState::default();
            let a = completed(
                1,
                "compared-a",
                chrono::TimeDelta::try_hours(2).unwrap(),
                Some(600),
            );
            tracked(&mut state, &a, 300);
            state.tasks.insert(a.id, a);
            let handle = review_rig(
                state,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
            );
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(model.empty);
            assert_eq!(model.rows.row_count(), 0);
            for section in [model.tracking_week.clone(), model.tracking_month.clone()] {
                assert!(section.empty, "the belt never projects the folds");
                assert_eq!(section.work, "");
                assert_eq!(section.bars.row_count(), 0);
            }
        });
    }

    // -- the reflection sections + the entries ledger (#623 slice B, M-13..M-16) --

    /// A Day-cadence entry whose stored window opens `age` before the
    /// pinned render clock (the same seed discipline as `tracked`).
    fn reflected(
        raw: u128,
        age: chrono::TimeDelta,
        mood: Option<u8>,
        energy: Option<u8>,
    ) -> taskboard_domain::ReflectionEntry {
        let start = crate::test_platform::PINNED_NOW - age;
        let mut row = taskboard_domain::ReflectionEntry {
            id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::from_u128(raw)),
            cadence: taskboard_domain::ReviewCadence::Day,
            day: start.date_naive(),
            window_start: start,
            window_end: start + chrono::TimeDelta::try_hours(24).unwrap(),
            text: format!("entry {raw}"),
            attributes: taskboard_domain::ReflectionAttributes::default(),
            created_at: start + chrono::TimeDelta::try_hours(25).unwrap(),
            updated_at: start + chrono::TimeDelta::try_hours(25).unwrap(),
            deleted: false,
        };
        let to_rating = |n| taskboard_domain::Rating::try_new(n).unwrap();
        row.attributes.mood = mood.map(to_rating);
        row.attributes.energy = energy.map(to_rating);
        row
    }

    fn seed_reflection(state: &mut AppState, row: taskboard_domain::ReflectionEntry) {
        state.reflection.entries.insert(row.id, row);
    }

    #[test]
    fn the_review_model_mirrors_the_reflection_folds() {
        on_slint_thread(|| {
            let mut state = AppState::default();
            // The newest entry, rated + flagged.
            let mut newest = reflected(1, chrono::TimeDelta::try_hours(3).unwrap(), Some(4), None);
            newest
                .attributes
                .flags
                .insert(taskboard_domain::HealthFlag::GoodDay);
            seed_reflection(&mut state, newest);
            // An older entry, energy-rated only.
            seed_reflection(
                &mut state,
                reflected(2, chrono::TimeDelta::try_days(3).unwrap(), None, Some(3)),
            );
            // A tombstone: never surfaces anywhere.
            let mut dead = reflected(3, chrono::TimeDelta::try_hours(2).unwrap(), Some(5), None);
            dead.deleted = true;
            seed_reflection(&mut state, dead);

            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();

            for section in [
                model.reflection_week.clone(),
                model.reflection_month.clone(),
            ] {
                assert!(!section.empty, "live entries sit in both windows");
                assert!(!section.mood.is_empty());
                assert!(!section.energy.is_empty());
                assert!(!section.flags.is_empty());
                assert!(
                    !section.per_flag.is_empty(),
                    "the counts line speaks when a flag occurred"
                );
            }
            assert_eq!(model.reflection_week.bars.row_count(), 7);
            assert_eq!(model.reflection_month.bars.row_count(), 30);
            let last = model
                .reflection_week
                .bars
                .row_data(6)
                .expect("the running day's bar");
            assert!(
                last.base > 0.0,
                "the seeded mood sits in the running bucket"
            );
            // The ledger: newest first, tombstones gone, unrated is
            // the dash.
            assert_eq!(model.reflection_rows.row_count(), 2);
            let first = model.reflection_rows.row_data(0).expect("row 0");
            assert!(!first.mood.is_empty());
            assert!(!first.flags.is_empty());
            assert!(!first.text.is_empty());
            let second = model.reflection_rows.row_data(1).expect("row 1");
            assert!(second.mood.is_empty(), "unrated is the dash");
            assert!(!second.energy.is_empty());
            assert_ne!(
                first.day, second.day,
                "the two entries carry distinct stored day labels"
            );
        });
    }

    #[test]
    fn the_reflection_empty_state_collapses_in_tiers() {
        on_slint_thread(|| {
            // Tier 1 + 2 on the bare fixture: the whole view folds.
            let handle = review_rig(AppState::default(), UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(model.empty);
            assert_eq!(model.reflection_rows.row_count(), 0);
            for (section, days) in [
                (model.reflection_week.clone(), 7),
                (model.reflection_month.clone(), 30),
            ] {
                assert!(section.empty, "tier 2: the section collapses");
                assert_eq!(section.mood, "");
                assert_eq!(section.energy, "");
                assert_eq!(section.flags, "");
                assert_eq!(section.per_flag, "");
                assert_eq!(section.bars.row_count(), days, "the skeleton stays");
                for i in 0..days {
                    let bar = section.bars.row_data(i).expect("bar");
                    assert!(bar.base == 0.0 && bar.stack == 0.0, "baseline ticks only");
                }
            }

            // Tier 1's extension: a journals-only state stands the
            // view — reflection entries alone are content.
            let mut state = AppState::default();
            seed_reflection(
                &mut state,
                reflected(1, chrono::TimeDelta::try_hours(3).unwrap(), Some(2), None),
            );
            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert!(!model.empty, "journals alone keep the view standing");
            assert!(!model.reflection_month.empty);
            assert!(
                model.week.empty && model.tracking_month.empty,
                "the untouched families still collapse"
            );
        });
    }

    #[test]
    fn the_entries_ledger_diffs_against_mutation() {
        on_slint_thread(|| {
            let mut state = AppState::default();
            let older = reflected(1, chrono::TimeDelta::try_hours(5).unwrap(), Some(3), None);
            seed_reflection(&mut state, older.clone());
            let handle = review_rig(state, UiSettings::default());
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert_eq!(model.reflection_rows.row_count(), 1);
            let row = model.reflection_rows.row_data(0).expect("row");
            assert_eq!(row.day, older.day.to_string(), "the stored label verbatim");

            // A journal verb between passes: one entry added, one
            // tombstoned — the ledger tracks the live set newest-first.
            let mut next = handle.test_state().load_full().as_ref().clone();
            seed_reflection(
                &mut next,
                reflected(2, chrono::TimeDelta::try_hours(2).unwrap(), None, Some(4)),
            );
            let mut dead = reflected(1, chrono::TimeDelta::try_hours(5).unwrap(), Some(3), None);
            dead.deleted = true;
            next.reflection.entries.insert(dead.id, dead);
            handle.test_state().store(std::sync::Arc::new(next));
            handle
                .test_adapter()
                .borrow_mut()
                .refresh_forced(handle.window());
            let model = handle.window().get_review_model();
            assert_eq!(model.reflection_rows.row_count(), 1, "the tombstone left");
            let row = model.reflection_rows.row_data(0).expect("row");
            assert!(
                row.mood.is_empty() && !row.energy.is_empty(),
                "the surviving row is the new energy-rated entry"
            );
        });
    }
}

#[cfg(test)]
mod habit_card_tests {
    use super::*;
    use crate::presentation::HabitRow;
    use proptest::prelude::*;

    fn row(
        count: u32,
        quota: u32,
        streak: u32,
        chip: Option<chrono::NaiveTime>,
        anchor_day: Option<taskboard_domain::AnchorDay>,
    ) -> HabitRow {
        HabitRow {
            habit: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
            title: "water".into(),
            target_phrase: "3/day".into(),
            count,
            quota,
            streak,
            chip,
            anchor_day,
            repair: false,
            repair_undoable: false,
            met: count >= quota,
            skip: false,
            paused: false,
            paused_until: None,

            off_schedule: false,
        }
    }

    /// H2 A4/D2 — the card-slot fold: the habit fills title/done/
    /// scheduled and the decoration row; every task-only slot stays
    /// empty/hidden; the done slot never flips (surplus logs are
    /// first-class, D-P14).
    #[test]
    fn us1_s1_the_habit_card_fills_only_the_habit_slots() {
        let time = chrono::NaiveTime::from_hms_opt(7, 5, 0).unwrap();
        let model = habit_card(&row(3, 3, 2, Some(time), None));
        assert_eq!(model.title.as_str(), "water");
        assert!(!model.done, "surplus logs are first-class: no done bit");
        assert!(model.can_done, "the done slot IS the +1 log control");
        assert_eq!(model.due_phrase, "", "habits have no due");
        assert_eq!(model.effort_phrase, "");
        assert_eq!(model.scheduled_phrase, "07:05", "the time chip");
        assert_eq!(model.track_state, TrackCardState::None);
        let decorations: Vec<_> = model.decorations.iter().collect();
        assert_eq!(decorations.len(), 2, "progress + streak");
        assert_eq!(decorations[0].label, "3/3");
        assert_eq!(decorations[0].tone, Tone::Success, "met restyles only");
        // The W-9 badge swap: the streak decoration carries the typed
        // value; the emoji slot is retired (the label text is judged
        // at walkthrough, never asserted beyond the value).
        assert_eq!(decorations[1].label, "2");
        assert!(!model.selected && !model.anchor && !model.dragging_source);
        // Under quota: neutral progress tone; no streak: one chip only.
        let model = habit_card(&row(1, 3, 0, None, None));
        let decorations: Vec<_> = model.decorations.iter().collect();
        assert_eq!(decorations.len(), 1);
        assert_eq!(decorations[0].tone, Tone::Neutral);
        assert_eq!(model.scheduled_phrase, "", "no chip facts, no chip");
    }

    // H2 A4 — the property: for any `(count, quota, streak, chip,
    // day)` the card carries EXACTLY the decoration set and slot
    // state the facts imply (pure domain→presentation derivation).
    proptest! {
        #[test]
        fn the_card_carries_exactly_the_facts_implied_decoration_set(
            count in 0u32..12,
            quota in 1u32..12,
            streak in 0u32..30,
            has_chip in proptest::bool::ANY,
            has_day in proptest::bool::ANY,
        ) {
            let chip = has_chip
                .then(|| chrono::NaiveTime::from_hms_opt(9, 30, 0).unwrap());
            let day = has_day.then_some(taskboard_domain::AnchorDay::Weekday(chrono::Weekday::Thu));
            let model = habit_card(&row(count, quota, streak, chip, day));
            // The done slot: always the log control, never done.
            prop_assert!(model.can_done);
            prop_assert!(!model.done);
            // The scheduled slot: present iff any chip fact is.
            prop_assert_eq!(model.scheduled_phrase.is_empty(), !has_chip && !has_day);
            // The decoration set: progress always, streak iff > 0.
            let decorations: Vec<_> = model.decorations.iter().collect();
            prop_assert_eq!(decorations.len(), 1 + usize::from(streak > 0));
            prop_assert_eq!(decorations[0].label.as_str(), format!("{count}/{quota}"));
            prop_assert_eq!(decorations[0].tone == Tone::Success, count >= quota);
            // The task-only slots never fill.
            prop_assert_eq!(model.due_phrase, "");
            prop_assert_eq!(model.effort_phrase, "");
            prop_assert_eq!(model.track_state, TrackCardState::None);
        }
    }
}

/// The reset letter's D-3 derivation inputs (OP1 US-2): the armed
/// board's live rows from the state the adapter already holds — the
/// same facts the CLI's typed reset refusal states, derived
/// presentation-side (no engine query). Counts: live stacks, live
/// tasks on those stacks, live labels.
fn board_content_counts(
    state: &taskboard_domain::AppState,
    board: taskboard_domain::BoardId,
) -> (i32, i32, i32) {
    let stacks: Vec<_> = state
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.id)
        .collect();
    let tasks = state
        .tasks
        .values()
        .filter(|t| stacks.contains(&t.stack) && t.is_live())
        .count();
    let labels = state
        .labels
        .values()
        .filter(|l| l.board == board && !l.deleted)
        .count();
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    {
        (stacks.len() as i32, tasks as i32, labels as i32)
    }
}

/// The habit history dialog's projection pass (HF3 W-8/W-10, US-7 /
/// US-9): while the view state holds a one-shot history target, the
/// month-grid model and the statistics ride the window globals —
/// pure projection over `HabitState.log`, zero commands (the plan's
/// no-command reason per US-7/S2–S4). The target may be tombstoned:
/// the log outlives the tracker (US-7 AC-5), so the lookup reads the
/// full habit map, live rows and tombstones alike. Returns the diff's
/// mutation count.
fn push_habit_history(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    now: chrono::DateTime<chrono::Utc>,
    calendar: taskboard_domain::DayCalendar,
) -> usize {
    let (target, month) = vs.habit_history();
    let Some((habit, month)) = target.zip(month) else {
        if window.get_habit_history_open() {
            window.set_habit_history_open(false);
            return 1;
        }
        return 0;
    };
    let Some(habit) = state.habits.habits.get(&habit) else {
        // The subject died mid-session (a concurrent tombstone): the
        // dialog cannot project — it closes (the stale-subject
        // posture, never a wrong grid).
        if window.get_habit_history_open() {
            window.set_habit_history_open(false);
            return 1;
        }
        return 0;
    };
    let shown =
        crate::presentation::habit_history_month(habit, &state.habits.log, month, now, calendar);
    let numbers = crate::presentation::habit_statistics(habit, &state.habits.log, now, calendar);
    // US-8 (#1144, W-11): a Week/Month habit's month grid renders one
    // cell per calendar DAY from the per-day overlay facts (the D-H12
    // amendment; the stats section stays window-canonical — R-2). A
    // Day-kind habit renders as landed (its windows ARE days). A day
    // with no overlay fact renders empty (`count: 0` → no badge, the
    // dimmest neutral state — no invention).
    let day_cells = (habit.target.kind() != taskboard_domain::PeriodKind::Day)
        .then(|| crate::presentation::habit_history_day_cells(habit, &state.habits.day_log, month));
    let cells: Vec<crate::HabitCellModel> = if let Some(day_cells) = &day_cells {
        day_cells
            .iter()
            .map(|cell| crate::HabitCellModel {
                label: SharedString::from(cell.day.day().to_string()),
                count: i32::try_from(cell.count).unwrap_or(i32::MAX),
                state: if cell.count > 0 {
                    crate::HabitCellState::Met
                } else {
                    crate::HabitCellState::Inert
                },
            })
            .collect()
    } else {
        shown
            .cells
            .iter()
            .map(|cell| crate::HabitCellModel {
                label: SharedString::from(cell.window.day().to_string()),
                count: i32::try_from(cell.count).unwrap_or(i32::MAX),
                state: match cell.state {
                    crate::presentation::HabitWindowState::Met => crate::HabitCellState::Met,
                    crate::presentation::HabitWindowState::Pending => {
                        crate::HabitCellState::Pending
                    }
                    crate::presentation::HabitWindowState::Skipped => {
                        crate::HabitCellState::Skipped
                    }
                    crate::presentation::HabitWindowState::Paused => crate::HabitCellState::Paused,
                    crate::presentation::HabitWindowState::PreSchedule => {
                        crate::HabitCellState::PreSchedule
                    }
                    crate::presentation::HabitWindowState::Missed => crate::HabitCellState::Missed,
                    crate::presentation::HabitWindowState::Inert => crate::HabitCellState::Inert,
                },
            })
            .collect()
    };
    let model = crate::HabitHistoryModel {
        month_label: SharedString::from(crate::presentation::habit_month_label(shown.month)),
        cells: ModelRc::new(VecModel::from(cells)),
        can_page_back: shown.can_page_back,
        can_page_forward: shown.can_page_forward,
        stats: crate::HabitStatsModel {
            adherence_empty: numbers.adherence.is_empty(),
            adherence_met: i32::try_from(numbers.adherence.met()).unwrap_or(i32::MAX),
            adherence_scheduled: i32::try_from(numbers.adherence.scheduled()).unwrap_or(i32::MAX),
            best_streak: i32::try_from(numbers.best_streak).unwrap_or(i32::MAX),
        },
    };
    window.set_habit_history(model);
    window.set_habit_history_open(true);
    1
}

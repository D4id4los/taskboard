// SPDX-License-Identifier: AGPL-3.0-only

//! The generated action menu (chunk decision D19, design §5.9; the
//! round-1 two-tier rework, D5–D7/D10): pure projections of the
//! registry over keymap data for a context chain. Menus, hints, and
//! descriptions are projections of data, never hand-written (§5.9).
//! The presentation-only `menu_rank` / `menu_group` columns order the
//! pages; keymap resolution never reads them (resolution is built
//! from the preset tables alone, so the "resolution order untouched"
//! guarantee is structural).

use crate::actions::{ActionId, ContextId, MenuGroup, REGISTRY};
use crate::keymap::Keymap;

/// One generated menu row (D19, extended by round-1 D5/D7): the
/// registry entry plus its first chain-visible binding rendered as
/// canonical display strings (the kbd-hint text source). Disabled
/// entries are included — menu generation stays total and documents
/// the reservations (§5.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    /// `None` for a synthesized submenu-parent row (it never
    /// dispatches; activating it opens the group's page).
    pub action: Option<ActionId>,
    pub label: &'static str,
    /// The row's tooltip / a11y description (round-1 D7); every
    /// registry row carries one, parent rows too.
    pub description: &'static str,
    pub enabled: bool,
    /// The first chain-visible binding, one display string per stroke.
    pub hint: Vec<String>,
    /// `Some(submenu)` for a submenu-parent row (round-1 D6; UX-R8
    /// chunk-5 D-5 widens the shape with the task families).
    pub submenu: Option<Submenu>,
}

/// A submenu-parent row's target (UX-R8 chunk-5 D-5): the main menu's
/// `MenuGroup` pages, or the task context menu's curated families.
/// NOT `MenuGroup` variants — new `MenuGroup` rows would leak parent
/// rows onto the main root page, whose `root_page` iterates
/// `MenuGroup::all()`; the main menu's Task page stays exactly as
/// landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Submenu {
    Group(MenuGroup),
    Task(TaskMenuGroup),
}

/// The task context menu's rare families (UX-R8 chunk-5 D-5, US-2):
/// Convert ▸ (the habit/series verbs), Reminders ▸, Timers ▸, and
/// Entries ▸ — each child row keeps its existing `ActionId`, hint,
/// and shortcut projection (the T-3 contract per family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskMenuGroup {
    Convert,
    Reminders,
    Timers,
    Entries,
}

impl TaskMenuGroup {
    /// The declaration order the parents render in.
    pub const ALL: &'static [TaskMenuGroup] = &[
        TaskMenuGroup::Convert,
        TaskMenuGroup::Reminders,
        TaskMenuGroup::Timers,
        TaskMenuGroup::Entries,
    ];

    /// The parent row's label (the element's address, D16).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            TaskMenuGroup::Convert => "Convert",
            TaskMenuGroup::Reminders => "Reminders",
            TaskMenuGroup::Timers => "Timers",
            TaskMenuGroup::Entries => "Entries",
        }
    }
}

/// The task context menu's per-subject facts (UX-R8 chunk-5 D-4,
/// US-2): the state/kind inputs the withheld-vs-greyed verdicts
/// derive from. Derived at open from the loaded `AppState` (see
/// `presentation::task_menu_subject`); a plain data carrier — menu.rs
/// stays projection-only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one fact per field — the verdict table reads each by name (the plan's D-4 shape)
pub struct TaskMenuSubject {
    /// The subject holds THE open tracking entry.
    pub tracked_here: bool,
    /// Another task's open tracking entry.
    pub tracking_elsewhere: bool,
    /// Any reminder rule row (or reminder state) on the subject.
    pub has_reminder_rules: bool,
    /// The subject is a recurring series (`recur.is_some()`).
    pub is_series: bool,
    /// The subject's kind is Habit.
    pub is_habit: bool,
    /// The subject's kind is Tracker (the #492 filter).
    pub is_tracker: bool,
    /// The subject holds an armed timer. The domain's alarms are
    /// standalone (`AdhocAlarm` anchors no task), so no task-anchored
    /// timer exists today — the fact derives `false` and the
    /// `TimerCancel` row withholds on every task menu (the C-4 law:
    /// the row routed `NoTarget` from this menu anyway).
    pub has_armed_timer: bool,
    /// The subject carries a scheduled day.
    pub scheduled: bool,
}

/// The preset-irrelevant filter (round-3 D27b): the Mode group
/// renders only when the active keymap binds `mode.insert` in
/// `ViNormal` — true exactly for the vi preset. Data-derived, no new
/// plumbing; the group-page emptiness cascade hides the parent row
/// with the page.
fn preset_irrelevant(group: MenuGroup, keymap: &Keymap) -> bool {
    group == MenuGroup::Mode
        && !keymap
            .bound_contexts(crate::actions::ActionId::ModeInsert)
            .contains(&ContextId::ViNormal)
}

/// The registry rows whose availability intersects `chain`
/// (`Global`-tagged always), as `(registry index, entry)` in registry
/// order — the shared filter of all three page projections.
fn chain_visible(chain: &[ContextId]) -> Vec<(usize, &'static crate::actions::ActionEntry)> {
    REGISTRY
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry.contexts.contains(&ContextId::Global)
                || entry.contexts.iter().any(|c| chain.contains(c))
        })
        .collect()
}

fn row(
    keymap: &Keymap,
    chain: &[ContextId],
    states: &std::collections::BTreeMap<ActionId, bool>,
    entry: &crate::actions::ActionEntry,
) -> MenuEntry {
    MenuEntry {
        action: Some(entry.id),
        label: entry.label,
        description: entry.description,
        // Dynamic greying (round-3 D27c): an adapter-supplied state
        // override wins; the static registry `enabled` stays the
        // default (the permanently-disabled reservations' letter).
        enabled: states.get(&entry.id).copied().unwrap_or(entry.enabled),
        hint: keymap
            .first_binding_display(entry.id, chain)
            .unwrap_or_default(),
        submenu: None,
    }
}

/// The root page (round-1 D5/D10): the top-level quick rows
/// (`menu_group == None`, registry visibility over `chain`) in
/// `menu_rank` order, then one submenu-parent row per group whose
/// page is non-empty for the chain (group declaration order), then
/// `app.exit` pinned last regardless of its rank.
#[must_use]
pub fn root_page(
    keymap: &Keymap,
    chain: &[ContextId],
    states: &std::collections::BTreeMap<ActionId, bool>,
) -> Vec<MenuEntry> {
    let visible = chain_visible(chain);
    let mut quick: Vec<(u16, usize, MenuEntry)> = visible
        .iter()
        .filter(|(_, entry)| entry.menu_group.is_none() && entry.id != ActionId::AppExit)
        .map(|(index, entry)| (entry.menu_rank, *index, row(keymap, chain, states, entry)))
        .collect();
    quick.sort_by_key(|(rank, index, _)| (*rank, *index));

    let mut page: Vec<MenuEntry> = quick.into_iter().map(|(_, _, entry)| entry).collect();
    for group in MenuGroup::all() {
        if preset_irrelevant(*group, keymap) {
            continue;
        }
        if !group_page(keymap, chain, states, *group).is_empty() {
            page.push(MenuEntry {
                action: None,
                label: group.label(),
                description: "Opens this group of actions.",
                enabled: true,
                hint: Vec::new(),
                submenu: Some(Submenu::Group(*group)),
            });
        }
    }
    if let Some((_, exit)) = visible
        .iter()
        .find(|(_, entry)| entry.id == ActionId::AppExit)
    {
        page.push(row(keymap, chain, states, exit));
    }
    page
}

/// One group's page (round-1 D6): the group's chain-visible rows in
/// `(menu_rank, registry order)`.
#[must_use]
pub fn group_page(
    keymap: &Keymap,
    chain: &[ContextId],
    states: &std::collections::BTreeMap<ActionId, bool>,
    group: MenuGroup,
) -> Vec<MenuEntry> {
    if preset_irrelevant(group, keymap) {
        return Vec::new();
    }
    let mut rows: Vec<(u16, usize, MenuEntry)> = chain_visible(chain)
        .iter()
        .filter(|(_, entry)| entry.menu_group == Some(group))
        .map(|(index, entry)| (entry.menu_rank, *index, row(keymap, chain, states, entry)))
        .collect();
    rows.sort_by_key(|(rank, index, _)| (*rank, *index));
    rows.into_iter().map(|(_, _, entry)| entry).collect()
}

/// The task context menu's frequent core (UX-R8 chunk-5 D-5): the
/// rows that stay top-level for every applicable subject — everything
/// else in `TASK_CONTEXT` belongs to a family (the `Reminders ▸` etc.
/// pages). The destructive delete is pinned last by
/// [`task_context_page`], not listed here.
const TASK_CURATED_CORE: &[ActionId] = &[
    ActionId::TaskCreate,
    ActionId::TaskEdit,
    ActionId::TaskComplete,
    ActionId::TaskReopen,
    ActionId::TaskMove,
    ActionId::TaskMoveCancel,
    ActionId::TaskScheduleToday,
    ActionId::TaskScheduleTomorrow,
    ActionId::TaskUnschedule,
    ActionId::TaskTrackStart,
    ActionId::TaskTrackStop,
    ActionId::TaskToggleTracker,
];

/// One family's curated rows (UX-R8 chunk-5 D-5): the family's
/// registry rows through the applicable filter, in `(menu_rank,
/// registry order)` — the same `row()` projection as every page (the
/// T-3 hint contract holds per family).
#[must_use]
pub fn task_submenu_page(
    keymap: &Keymap,
    chain: &[ContextId],
    subject: TaskMenuSubject,
    states: &std::collections::BTreeMap<ActionId, bool>,
    family: TaskMenuGroup,
) -> Vec<MenuEntry> {
    let ids = task_family_ids(subject, family);
    let mut rows: Vec<(u16, usize, MenuEntry)> = chain_visible(chain)
        .iter()
        .filter(|(_, entry)| ids.contains(&entry.id))
        .map(|(index, entry)| {
            let mut projected = row(keymap, chain, states, entry);
            // The applicable-but-blocked verdict (UX-R8 chunk-5 D-4):
            // `ReminderMute` greys — never withholds — without rules
            // (the origin AC-2's "stay").
            if entry.id == ActionId::ReminderMute && !subject.has_reminder_rules {
                projected.enabled = false;
            }
            (entry.menu_rank, *index, projected)
        })
        .collect();
    rows.sort_by_key(|(rank, index, _)| (*rank, *index));
    rows.into_iter().map(|(_, _, entry)| entry).collect()
}

/// A family's applicable row ids (UX-R8 chunk-5 D-4's per-family
/// verdicts): inapplicable rows are WITHHELD (dropped), applicable-
/// but-blocked rows stay greyed (the states map's `false` override).
fn task_family_ids(subject: TaskMenuSubject, family: TaskMenuGroup) -> Vec<ActionId> {
    match family {
        TaskMenuGroup::Reminders => {
            let mut ids = Vec::new();
            if subject.has_reminder_rules {
                ids.extend([
                    ActionId::ReminderSnooze,
                    ActionId::ReminderUnmute,
                    ActionId::ReminderRuleRemove,
                ]);
            }
            // `ReminderMute` greys (not withholds) without rules — the
            // applicable verb, precondition-blocked (the origin AC-2's
            // "stay"); the greying rides the states map.
            ids.push(ActionId::ReminderMute);
            ids.push(ActionId::ReminderRuleAdd);
            ids
        }
        TaskMenuGroup::Timers => {
            // `TimerCreate` always (the dispatch-time refusal is the
            // backstop); `TimerCancel` withholds without an armed
            // timer (see `TaskMenuSubject::has_armed_timer`).
            let mut ids = vec![ActionId::TimerCreate];
            if subject.has_armed_timer {
                ids.push(ActionId::TimerCancel);
            }
            ids
        }
        TaskMenuGroup::Entries => vec![ActionId::TaskTrackEntries],
        TaskMenuGroup::Convert => {
            let mut ids = Vec::new();
            if subject.is_habit {
                ids.extend([
                    ActionId::HabitRepair,
                    ActionId::HabitConvertArm,
                    ActionId::HabitConvertToSeries,
                ]);
            }
            if subject.is_series {
                ids.extend([
                    ActionId::SeriesPause,
                    ActionId::SeriesPauseCommit,
                    ActionId::SeriesResume,
                    ActionId::SeriesConvertArm,
                    ActionId::SeriesConvertToHabit,
                ]);
            }
            ids
        }
    }
}

/// The task-card context menu's curated page (UX-R8 chunk-5 D-5,
/// US-2): the quick bar (D-2) + the frequent core rows (create, edit,
/// complete/reopen, move, move-cancel, the schedule trio, the
/// tracking twin) in registry order → one submenu-parent row per
/// family whose child page is non-empty for the subject (the
/// empty-page cascade) → destructive rows last (BF-W-N2). Returns
/// `(bar, rows)`. `extra` extends the core section (the today seam's
/// `TaskDueToday`). Deviation from the plan's signature: the `states`
/// map rides along — the flow-grey verdicts (`TaskMoveCancel`, the
/// live pause arm) are state facts the projection must read.
#[must_use]
#[allow(clippy::too_many_lines)] // the verdict table spelled once
pub fn task_context_page(
    keymap: &Keymap,
    chain: &[ContextId],
    subject: TaskMenuSubject,
    states: &std::collections::BTreeMap<ActionId, bool>,
    extra: &[ActionId],
) -> (Vec<QuickBarEntry>, Vec<MenuEntry>) {
    let bar = task_quick_bar(
        subject.scheduled,
        subject.tracked_here,
        subject.tracking_elsewhere,
    );

    // The core section: TASK_CONTEXT's registry-order walk (the T-12
    // source of truth) minus the submenu families, through the
    // per-row verdicts.
    let mut rows: Vec<(u16, usize, MenuEntry)> = Vec::new();
    let mut delete_row: Option<MenuEntry> = None;
    for (index, entry) in chain_visible(chain) {
        let id = entry.id;
        if !TASK_CURATED_CORE.contains(&id) && !extra.contains(&id) && id != ActionId::TaskDelete {
            continue;
        }
        // The verdict table (D-4): withheld beats the projection.
        let withheld = match id {
            ActionId::TaskComplete => subject.is_tracker,
            // The twin's shown half is the bar's (D-4's "bar + rows"
            // verdicts agree): Start shows until the subject holds
            // THE open entry (elsewhere-tracking greys it via the
            // states map's singleton precondition), Stop shows only
            // then.
            ActionId::TaskTrackStart => subject.tracked_here,
            ActionId::TaskTrackStop => !subject.tracked_here,
            _ => false,
        };
        if withheld {
            continue;
        }
        let projected = row(keymap, chain, states, entry);
        if id == ActionId::TaskDelete {
            // BF-W-N2: destructive last — pinned out of the rank walk.
            delete_row = Some(projected);
        } else {
            rows.push((entry.menu_rank, index, projected));
        }
    }
    rows.sort_by_key(|(rank, index, _)| (*rank, *index));
    let mut page: Vec<MenuEntry> = rows.into_iter().map(|(_, _, entry)| entry).collect();
    // The extras (the today seam's due-today twin): after the core
    // rank walk, before the parents.
    for id in extra {
        if let Some(entry) = REGISTRY.iter().find(|entry| entry.id == *id) {
            page.push(row(keymap, chain, states, entry));
        }
    }
    // One parent per rendered family (the empty-page cascade: a
    // rule-less subject renders no Reminders ▸ parent at all).
    for family in TaskMenuGroup::ALL {
        if task_submenu_page(keymap, chain, subject, states, *family).is_empty() {
            continue;
        }
        page.push(MenuEntry {
            action: None,
            label: family.label(),
            description: "Opens this group of actions.",
            enabled: true,
            hint: Vec::new(),
            submenu: Some(Submenu::Task(*family)),
        });
    }
    if let Some(delete) = delete_row {
        page.push(delete);
    }
    (bar, page)
}

/// The task-card context menu (round-1 D13, round-2 D21 spelling):
/// the Task group's chain-visible rows, flat (no parents), no Quit
/// pin. The reopen and move-cancel rows join automatically with their
/// Task-group registry rows.
#[must_use]
pub fn context_entries(keymap: &Keymap, chain: &[ContextId]) -> Vec<MenuEntry> {
    group_page(
        keymap,
        chain,
        &std::collections::BTreeMap::new(),
        MenuGroup::Task,
    )
}

/// The multi-selection menu's row set (UX-R8 D-6, US-4/S3, W-5): the
/// multi-sensible verbs only — complete then delete (the BF-W-N2
/// destructive-last rule is the row order). Single-target-only rows
/// (tracking arms, series/habit verbs, open/entries, and chunk 5's
/// quick schedule family) are **withheld, not greyed** (C-4). Chunk 5
/// builds the menu grammar (the applicable-row filter + submenus);
/// this named const is the row set the selection branch hands it —
/// the named-const-slice pattern, never a re-pasted row block.
pub const TASK_CONTEXT_MULTI: &[ActionId] = &[ActionId::TaskComplete, ActionId::TaskDelete];

/// The habit row context menu's curated core (H2 D4): the top-level
/// rows in declared order — log, undo, the make-up valve, the
/// reorder pair (the menu stopgap until H3's drag/keys reorder), the
/// Convert arm. The Convert confirm rides the submenu (the existing
/// `TaskMenuGroup::Convert` machinery, reused not duplicated); the
/// edit/delete rows are ABSENT — reserved verbs of H4 (`HabitEdit`
/// has no dead rows; #944's staged confirm lands there), so the
/// destructive-last law has no delete row to pin yet.
pub const HABIT_CONTEXT: &[ActionId] = &[
    ActionId::HabitLog,
    ActionId::HabitSkip,
    ActionId::HabitPause,
    ActionId::HabitResume,
    ActionId::HabitUndoLog,
    ActionId::HabitRepair,
    ActionId::HabitMoveUp,
    ActionId::HabitMoveDown,
    ActionId::HabitEdit,
    ActionId::HabitHistory,
    ActionId::HabitConvertArm,
    ActionId::HabitRemove,
    ActionId::HabitRemoveConfirm,
    ActionId::HabitRemoveCancel,
    // The undo-family restore trio (W-3): the context menu's restore
    // rows route the roster-addressed resurrections.
    ActionId::HabitRestore,
    ActionId::HabitRestoreConfirm,
    ActionId::HabitRestoreCancel,
];

/// The habit row context menu's page (H2 D4, US-2): the curated core
/// rows in declared order over the one object-menu chain, then the
/// Convert ▸ parent row (the empty-page cascade — a non-habit
/// subject renders none). `restore` is the restorable habit's name
/// (the roster's newest tombstone) when the roster is non-empty and
/// the platform is desktop (W-3, D-4) — `None` renders no restore
/// rows at all. Deviations from the plan's spelling,
/// recorded: (1) the armed-confirm state rides the `states` map —
/// `states[HabitConvertToSeries] == true` IS "armed", the
/// `SeriesPauseCommit` flow-grey precedent — instead of a separate
/// `armed: bool`; (2) the armed cancel step is a `HabitConvertCancel`
/// child row appended to the Convert page while armed, and the
/// reorder pair mints its slot through the direction-bearing
/// `HabitMoveUp`/`HabitMoveDown` ids (a single `HabitMove` id cannot
/// carry the direction through the one-shot grammar). The `subject`
/// facts come from `task_menu_subject` widened over `RowSubject`
/// (D1): `is_habit` gates the Convert family exactly as the task
/// page's family loop does.
#[must_use]
pub fn habit_context_page(
    keymap: &Keymap,
    chain: &[ContextId],
    subject: TaskMenuSubject,
    states: &std::collections::BTreeMap<ActionId, bool>,
    restore: Option<&str>,
) -> Vec<MenuEntry> {
    let mut rows: Vec<(u16, usize, MenuEntry)> = Vec::new();
    for (index, entry) in chain_visible(chain) {
        let id = entry.id;
        if !HABIT_CONTEXT.contains(&id) {
            continue;
        }
        // The remove trio is destructive-last: appended after the
        // Convert parent below, never ranked into the body (D4).
        if matches!(
            id,
            ActionId::HabitRemove | ActionId::HabitRemoveConfirm | ActionId::HabitRemoveCancel
        ) || matches!(
            id,
            ActionId::HabitRestore | ActionId::HabitRestoreConfirm | ActionId::HabitRestoreCancel
        ) {
            continue;
        }
        let projected = row(keymap, chain, states, entry);
        rows.push((entry.menu_rank, index, projected));
    }
    rows.sort_by_key(|(rank, index, _)| (*rank, *index));
    let mut page: Vec<MenuEntry> = rows.into_iter().map(|(_, _, entry)| entry).collect();
    // The Convert ▸ parent (the empty-page cascade): its children are
    // the existing habit Convert family plus — while armed — the
    // cancel step (the armed-confirm lives INSIDE the menu, O-7 Q3).
    if !habit_convert_children(keymap, chain, subject, states, TaskMenuGroup::Convert).is_empty() {
        page.push(MenuEntry {
            action: None,
            label: TaskMenuGroup::Convert.label(),
            description: "Opens this group of actions.",
            enabled: true,
            hint: Vec::new(),
            submenu: Some(Submenu::Task(TaskMenuGroup::Convert)),
        });
    }
    // The undo-family restore's page rows (W-3): BEFORE the staged
    // remove (BF-W-N2 — the destructive trio stays last). The arm row
    // is bespoke so its label names the restorable habit (the roster's
    // row is the contract — the UI renders, never synthesizes); the
    // confirm flips with the arm; the cancel joins armed-only. Kiosk
    // passes `restore: None` (D-4: the affordance renders absent, the
    // CLI carries the kiosk restore path).
    if restore.is_some() {
        let armed = states
            .get(&ActionId::HabitRestoreConfirm)
            .copied()
            .unwrap_or(false);
        if let Some(entry) = REGISTRY
            .iter()
            .find(|entry| entry.id == ActionId::HabitRestore)
        {
            let mut projected = row(keymap, chain, states, entry);
            // The registry label stays static (MenuEntry's &'static str
            // law); the armed strip's consequence names the habit via
            // the adapter's notice lane (the naming half is the stage-3
            // walkthrough's Manual item).
            projected.enabled = states.get(&ActionId::HabitRestore).copied().unwrap_or(true);
            page.push(projected);
        }
        for id in [ActionId::HabitRestoreConfirm, ActionId::HabitRestoreCancel] {
            if let Some(entry) = REGISTRY.iter().find(|entry| entry.id == id) {
                let projected = row(keymap, chain, states, entry);
                if id == ActionId::HabitRestoreCancel && !armed {
                    continue; // the cancel step joins while armed only
                }
                page.push(projected);
            }
        }
    }
    // The staged remove's page rows (H4 D4, #944), destructive-last:
    // the arm, the confirm (greyed while unarmed — consequence-first,
    // it flips enabled with the arm), and — while armed — the cancel
    // step. `states[HabitRemoveConfirm] == true` IS armed (the
    // convert trio's states-map grammar, deviation (1)).
    for id in [
        ActionId::HabitRemove,
        ActionId::HabitRemoveConfirm,
        ActionId::HabitRemoveCancel,
    ] {
        if let Some(entry) = REGISTRY.iter().find(|entry| entry.id == id) {
            let projected = row(keymap, chain, states, entry);
            if id == ActionId::HabitRemoveCancel && !states.get(&id).copied().unwrap_or(false) {
                continue; // the cancel step joins while armed only
            }
            page.push(projected);
        }
    }
    page
}

/// One habit menu family's children (H2 D4): the Convert family is
/// the existing habit arm through `task_submenu_page` (the machinery
/// reused, not duplicated), plus the armed cancel step while the
/// states map carries the armed verdict. Any other family falls
/// through to the shared projection (empty for habit subjects).
#[must_use]
pub fn habit_convert_children(
    keymap: &Keymap,
    chain: &[ContextId],
    subject: TaskMenuSubject,
    states: &std::collections::BTreeMap<ActionId, bool>,
    family: TaskMenuGroup,
) -> Vec<MenuEntry> {
    if family != TaskMenuGroup::Convert {
        return task_submenu_page(keymap, chain, subject, states, family);
    }
    let mut children = task_submenu_page(keymap, chain, subject, states, family);
    if states
        .get(&ActionId::HabitConvertToSeries)
        .copied()
        .unwrap_or(false)
        && let Some(entry) = REGISTRY
            .iter()
            .find(|entry| entry.id == ActionId::HabitConvertCancel)
    {
        children.push(row(keymap, chain, states, entry));
    }
    children
}

/// The task context menu's action list (round-2 D21): the Task
/// group's rows in rank order.
pub const TASK_CONTEXT: &[ActionId] = &[
    ActionId::TaskCreate,
    ActionId::TaskEdit,
    ActionId::TaskComplete,
    ActionId::TaskReopen,
    ActionId::TaskMove,
    ActionId::TaskMoveCancel,
    ActionId::TaskDelete,
    // UX-R8 chunk-5 D-3/D-11: the scheduled field's quick verbs —
    // ranks 101/102/103, between the delete and the reminder block.
    ActionId::TaskScheduleToday,
    ActionId::TaskScheduleTomorrow,
    ActionId::TaskUnschedule,
    ActionId::ReminderSnooze,
    ActionId::ReminderMute,
    ActionId::ReminderUnmute,
    ActionId::ReminderRuleAdd,
    ActionId::ReminderRuleRemove,
    // DR-3/3: the timers family — the Task group page's rows (the
    // object menu mirrors the group page; an invocation without a
    // timer target routes NoTarget).
    ActionId::TimerCreate,
    ActionId::TimerCancel,
    // DR-5/4: the tracking arms — pointer-driven, menu-reachable, no
    // preset bindings (the timers note verbatim); dispatch-time
    // gating only (the planner's NoOps own the refusals).
    ActionId::TaskTrackStart,
    ActionId::TaskTrackStop,
    // Issue #492: the kind toggle — pointer-driven, menu-reachable,
    // no preset binding (the tracking arms' note verbatim); dispatch
    // reads the kind and inverts it through the plain kind edit.
    ActionId::TaskToggleTracker,
    // The entries overlay (r4 chunk 10, D-TS8): the object menu's
    // Task-group mirror; the open arm's kiosk/subject refusals are
    // the dispatch-time gating.
    ActionId::TaskTrackEntries,
    // The habit trio (issue #506): the group page's mirror — the
    // verbs are pointer-driven, dispatch-time gated, no preset
    // bindings (the tracking arms' note verbatim).
    ActionId::HabitRepair,
    ActionId::HabitConvertArm,
    ActionId::HabitConvertToSeries,
    ActionId::SeriesPause,
    ActionId::SeriesPauseCommit,
    ActionId::SeriesResume,
    ActionId::SeriesConvertArm,
    ActionId::SeriesConvertToHabit,
];

/// The stack-header context menu's action list (round-2 D21): the
/// stack verbs plus new-task, destructive last, no parents, no Quit.
pub const STACK_CONTEXT: &[ActionId] = &[
    ActionId::TaskCreate,
    ActionId::StackMove,
    ActionId::StackRename,
    ActionId::StackWider,
    ActionId::StackNarrower,
    ActionId::StackDelete,
];

/// The board-bar segment context menu's action list (round-2 D21):
/// the create family, board rename, destructive last.
pub const BOARD_CONTEXT: &[ActionId] = &[
    ActionId::TaskCreate,
    ActionId::StackCreate,
    ActionId::BoardCreate,
    ActionId::BoardRename,
    // The color picker's opener (UX R-4 D-1, #791): beside the rename
    // row. The labels manager joins beside configuration (tag-color
    // plan D-7, US-2 — the roster-only overlay's opener); destructive
    // last.
    ActionId::BoardColorEdit,
    ActionId::LabelManage,
    ActionId::BoardDelete,
    // The board ceremony verbs (O-5 parity chunk OP1, US-1/US-2/US-3,
    // #945/#946/#947): the destructive tail's new rows (BF-W-N2 — the
    // informed-consent confirmations ride the ceremony fences; the r1
    // erratum law holds). Archive's typed escape is the roster
    // restore; the reset row is the tail's last (the most destructive
    // local repair); the reconcile row precedes it (the irreversible
    // merge above the total clear).
    ActionId::BoardArchive,
    ActionId::BoardReconcile,
    ActionId::BoardReset,
];

/// The quick-bar icon vocabulary (UX-R8 chunk-5 D-2, US-1): the
/// Rust-side half of the slint `IconKind` mapping — menu.rs stays
/// slint-free; the adapter's push maps it onto the generated enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickBarIcon {
    /// Schedule today (the shared calendar glyph).
    Calendar,
    /// Schedule tomorrow (calendar-plus — deliberately distinct, the
    /// F1 shared-icon lesson).
    CalendarPlus,
    /// Unschedule (calendar-minus).
    CalendarMinus,
    /// Start tracking (the play triangle).
    Play,
    /// Stop tracking (the stop square).
    Stop,
}

/// One quick-bar row (UX-R8 chunk-5 D-2, US-1): the routing id (click
/// ≡ key — the preset stroke routes the same `ActionId`), its icon,
/// and the shown/withheld verdict. `visible: false` is the C-4
/// withheld rule — the model still carries the row; the bar renders
/// only the shown half.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickBarEntry {
    pub action: ActionId,
    pub icon: QuickBarIcon,
    pub enabled: bool,
    pub visible: bool,
}

/// The task context menu's quick bar (UX-R8 chunk-5 D-2 over D-4's
/// per-family verdicts): schedule today/tomorrow always shown;
/// unschedule withheld on an unscheduled subject; the tracking twin's
/// shown half flips with the subject's tracking state (`tracked_here`
/// → Stop shown; `tracking_elsewhere` → Start shown greyed; neither
/// → Start shown). Built once at open; nothing re-derives it while
/// the menu holds (the D-1 pin covers the bar).
#[must_use]
pub fn task_quick_bar(
    scheduled: bool,
    tracked_here: bool,
    tracking_elsewhere: bool,
) -> Vec<QuickBarEntry> {
    let mut bar = vec![
        QuickBarEntry {
            action: ActionId::TaskScheduleToday,
            icon: QuickBarIcon::Calendar,
            enabled: true,
            visible: true,
        },
        QuickBarEntry {
            action: ActionId::TaskScheduleTomorrow,
            icon: QuickBarIcon::CalendarPlus,
            enabled: true,
            visible: true,
        },
        QuickBarEntry {
            action: ActionId::TaskUnschedule,
            icon: QuickBarIcon::CalendarMinus,
            enabled: true,
            visible: scheduled,
        },
    ];
    let (start_visible, stop_visible) = if tracked_here {
        (false, true)
    } else {
        (true, false)
    };
    bar.push(QuickBarEntry {
        action: ActionId::TaskTrackStart,
        icon: QuickBarIcon::Play,
        // The singleton precondition (the origin US-1/AC-4 shown-half
        // rule): another task's open entry greys Start.
        enabled: !tracking_elsewhere,
        visible: start_visible,
    });
    bar.push(QuickBarEntry {
        action: ActionId::TaskTrackStop,
        icon: QuickBarIcon::Stop,
        enabled: true,
        visible: stop_visible,
    });
    bar
}

/// The multi-selection context menu's quick bar (J5 US-2 over R-3's
/// multi-sensibility ruling): schedule today/tomorrow apply to every
/// member; unschedule shows when at least one member is scheduled
/// (the C-4 rule over aggregate facts); the tracking twin shows only
/// on a uniform selection (`tracked_members == count` → Stop;
/// `tracked_members == 0` → Start, greyed while another task tracks;
/// a mixed selection withholds the twin entirely — it is not
/// multi-sensible there). `count == 1` degenerates to the single-card
/// rule. No Complete/Delete icons — those stay menu rows
/// (`TASK_CONTEXT_MULTI`); the bar is quick-verbs only.
#[must_use]
pub fn multi_quick_bar(
    count: usize,
    any_scheduled: bool,
    tracked_members: usize,
    tracking_elsewhere: bool,
) -> Vec<QuickBarEntry> {
    let tracked_members = tracked_members.min(count);
    let uniform_tracked = tracked_members == count;
    let uniform_untracked = tracked_members == 0;
    let mut bar = vec![
        QuickBarEntry {
            action: ActionId::TaskScheduleToday,
            icon: QuickBarIcon::Calendar,
            enabled: true,
            visible: true,
        },
        QuickBarEntry {
            action: ActionId::TaskScheduleTomorrow,
            icon: QuickBarIcon::CalendarPlus,
            enabled: true,
            visible: true,
        },
        QuickBarEntry {
            action: ActionId::TaskUnschedule,
            icon: QuickBarIcon::CalendarMinus,
            enabled: true,
            visible: any_scheduled,
        },
    ];
    let (start_visible, stop_visible) = if uniform_tracked {
        (false, true)
    } else if uniform_untracked {
        (true, false)
    } else {
        (false, false) // mixed: the twin is withheld, not greyed
    };
    bar.push(QuickBarEntry {
        action: ActionId::TaskTrackStart,
        icon: QuickBarIcon::Play,
        // The singleton precondition rides over the aggregate: a
        // uniform-untracked selection still greys Start while another
        // task tracks.
        enabled: !tracking_elsewhere,
        visible: start_visible,
    });
    bar.push(QuickBarEntry {
        action: ActionId::TaskTrackStop,
        icon: QuickBarIcon::Stop,
        enabled: true,
        visible: stop_visible,
    });
    bar
}

/// One object context menu (round-2 D21): the named actions
/// projected through the same `row()` as every page — binding hints
/// included. Over `[Board, Global]` (the one chain every object menu
/// opens with) every board-surface verb's binding is visible, so the
/// F-B7 empty-hint bug cannot recur. No static enablement gating:
/// gating is dispatch-time (`NoTarget`/inert), the established
/// `task.complete`-on-no-focus machinery.
#[must_use]
pub fn object_entries(
    keymap: &Keymap,
    chain: &[ContextId],
    ids: &[ActionId],
    states: &std::collections::BTreeMap<ActionId, bool>,
) -> Vec<MenuEntry> {
    ids.iter()
        .filter_map(|id| REGISTRY.iter().find(|entry| entry.id == *id))
        .map(|entry| row(keymap, chain, states, entry))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionId;
    use crate::keymap::{Preset, RawKeymap, build};
    use std::collections::BTreeMap;

    fn standard_keymap() -> Keymap {
        build(&RawKeymap {
            preset: Preset::Standard,
            overrides: BTreeMap::new(),
        })
        .keymap
    }

    fn ids(rows: &[MenuEntry]) -> Vec<Option<ActionId>> {
        rows.iter().map(|e| e.action).collect()
    }

    fn board_chain() -> Vec<ContextId> {
        vec![ContextId::Board, ContextId::ViNormal, ContextId::Global]
    }

    /// T-1 — the curated root page: the five quick rows in rank
    /// order (the export dialog's data verb joins between the capture
    /// verb and the undo pair — the J2 export plan's D-1 menu entry
    /// point, rank 25), then the parent rows in group order (every
    /// group's page is non-empty over the board chain; six since the
    /// round-2 Stack/Board split, five since round-3 D27b filters the
    /// preset-irrelevant Mode group under the standard preset), then
    /// Quit last.
    #[test]
    fn root_page_is_curated_quick_rows_parents_quit() {
        let keymap = standard_keymap();
        let rows = root_page(&keymap, &board_chain(), &std::collections::BTreeMap::new());
        let mut actions = rows.iter().filter_map(|e| e.action);
        for id in [
            ActionId::AppSettings,
            ActionId::CaptureQuick,
            ActionId::ExportDialogOpen,
            ActionId::AppUndo,
            ActionId::AppRedo,
        ] {
            assert_eq!(actions.next(), Some(id), "quick row order");
        }
        // Then the parent rows (no action), then Quit. The Mode
        // group is filtered under the standard preset (round-3 D27b:
        // preset-irrelevant rows).
        let parents: Vec<_> = rows
            .iter()
            .filter(|e| e.action.is_none())
            .map(|e| e.submenu)
            .collect();
        let expected_parents: Vec<_> = MenuGroup::all()
            .iter()
            .copied()
            .filter(|g| *g != MenuGroup::Mode)
            .map(|g| Some(Submenu::Group(g)))
            .collect();
        assert_eq!(
            parents, expected_parents,
            "one parent per rendered group, in group order"
        );
        assert_eq!(
            *rows.last().expect("non-empty"),
            MenuEntry {
                action: Some(ActionId::AppExit),
                label: "Quit taskboard",
                description: "Quits the application; local state is already saved.",
                enabled: true,
                hint: vec!["Ctrl+Shift+q".to_owned()],
                submenu: None,
            },
            "Quit pinned last"
        );
    }

    /// T-2 — `app.exit` is the last root-page row for every chain.
    #[test]
    fn quit_is_last_on_every_root_page() {
        let keymap = standard_keymap();
        for chain in [
            vec![ContextId::Board, ContextId::ViNormal, ContextId::Global],
            vec![
                ContextId::StackDetail,
                ContextId::ViNormal,
                ContextId::Global,
            ],
            vec![
                ContextId::TaskDetail,
                ContextId::ViNormal,
                ContextId::Global,
            ],
            vec![ContextId::Global],
        ] {
            let rows = root_page(&keymap, &chain, &std::collections::BTreeMap::new());
            assert_eq!(
                rows.last().expect("non-empty").action,
                Some(ActionId::AppExit),
                "Quit last over {chain:?}"
            );
            assert_eq!(
                ids(&rows)
                    .iter()
                    .filter(|a| **a == Some(ActionId::AppExit))
                    .count(),
                1
            );
        }
    }

    /// US-23/S2 (`us23_s2_settings_row_present_on_kiosk_chain`,
    /// #1152): the reload lane's kiosk entry point — the `AppSettings`
    /// row is present AND enabled on the kiosk mount's chain (the
    /// structural pin: a registry row's presence and its enabled bit,
    /// never badge/caption text). `AppSettings` is absent from the
    /// adapter's `!self.kiosk` enabled-arms, so the kiosk chain
    /// inherits the same root-page row the desktop reads.
    #[test]
    fn us23_s2_settings_row_present_on_kiosk_chain() {
        let keymap = standard_keymap();
        let rows = root_page(&keymap, &board_chain(), &std::collections::BTreeMap::new());
        let settings = rows
            .iter()
            .find(|entry| entry.action == Some(ActionId::AppSettings))
            .expect("the AppSettings row renders on the kiosk chain");
        assert!(
            settings.enabled,
            "the reload lane's kiosk entry point is enabled"
        );
    }

    /// US-25/S1 (`us25_s1_root_quick_rows_within_budget`, #1184, R-3):
    /// the budget law's teeth, registry-wide.
    ///
    /// **Landing finding (2026-11-23)**: the accepted plan's premise
    /// — "the `menu_group: None` set is exactly the curated five" —
    /// measured false against the landed registry: the UF2 undo
    /// branch picker row (`AppUndoBranches`, rank 50) and the
    /// keyboard-chord families (HabitLog/HabitMove/FocusModeToggle/
    /// `BreakRule`…, ranks 190–730) are group-less GLOBAL rows behind
    /// the five, and 60+ context-bound verbs (the walk, pickers,
    /// editors, overlays) are group-less by design. G6 holds with the
    /// page as landed. The plan's R-3 text carried a dated erratum at
    /// landing; the re-curation question is queued upstream of this
    /// guard (see the D4 report / the #1184 comment). No surface
    /// change ships in D4.
    ///
    /// What the guard enforces instead, without weakening the teeth:
    /// 1. over every view chain the root page's quick rows LEAD with
    ///    the curated five in rank order (a higher-ranked interloper
    ///    fails here, before G6 ever measures a clipped border);
    /// 2. the group-less registry set is EXACTLY the enumerated
    ///    census below — a NEW group-less registration (global or
    ///    context-bound) fails here and must either join a group page
    ///    (the curation law's default) or amend this census.
    #[test]
    #[allow(clippy::too_many_lines)] // the census is the deliverable
    fn us25_s1_root_quick_rows_within_budget() {
        const CURATED_FIVE: &[ActionId] = &[
            ActionId::AppSettings,
            ActionId::CaptureQuick,
            ActionId::ExportDialogOpen,
            ActionId::AppUndo,
            ActionId::AppRedo,
        ];
        // The census: every `menu_group: None` row in the registry
        // (2026-11-23; the T-1 doc comment's per-row ranks are the
        // maintenance map). Dated absorption 2026-11-24: the sibling
        // chunks D1i (#1295 — the HabitRestore confirm trio) and D2
        // (#1288 — the Status panel remediation trio + the export
        // dialog's preview/copy pair) landed their group-less
        // internal verbs while this branch was in flight; the census
        // absorbs them, the guard's teeth are unchanged.
        const CENSUS: &[ActionId] = &[
            ActionId::AppExit,
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::AppUndoBranches,
            ActionId::AppSettings,
            ActionId::ExportDialogOpen,
            ActionId::ExportDialogClose,
            ActionId::CaptureQuick,
            ActionId::LabelOverlayDismiss,
            ActionId::BreakRuleCreate,
            ActionId::BreakRuleRemove,
            ActionId::BreakRuleRemoveConfirm,
            ActionId::AlarmDismiss,
            ActionId::AlarmSnooze,
            ActionId::AlarmCancel,
            ActionId::NotificationDismiss,
            ActionId::NotificationDone,
            ActionId::NotificationSnooze,
            ActionId::NotificationMute,
            ActionId::NotificationCancel,
            ActionId::NotificationTrackStart,
            ActionId::NotificationOpen,
            ActionId::RowMenu,
            ActionId::RowCursorDisarm,
            ActionId::TaskTrackEntryLog,
            ActionId::TaskTrackEntryAdjust,
            ActionId::TaskTrackEntryDelete,
            ActionId::TaskTrackEntryDismiss,
            ActionId::FocusModeToggle,
            ActionId::HabitLog,
            ActionId::HabitUndoLog,
            ActionId::HabitCreateCommit,
            ActionId::HabitScheduleEdit,
            ActionId::HabitReminderEdit,
            ActionId::HabitPauseCommit,
            ActionId::HabitRemoveCancel,
            ActionId::HabitRestore,
            ActionId::HabitRestoreConfirm,
            ActionId::HabitRestoreCancel,
            ActionId::HabitMoveUp,
            ActionId::HabitMoveDown,
            ActionId::HabitConvertCancel,
            ActionId::SeriesEditCommit,
            ActionId::SeriesEditDetach,
            ActionId::SeriesEditDetachArm,
            ActionId::SeriesEditDetachCancel,
            ActionId::PlanBeginPlace,
            ActionId::PlanCommitPlace,
            ActionId::PlanBoxEarlier,
            ActionId::PlanBoxLater,
            ActionId::PlanRemoveBox,
            ActionId::ReviewWalkCursorUp,
            ActionId::ReviewWalkCursorDown,
            ActionId::ReviewWalkBack,
            ActionId::ReviewWalkAdvance,
            ActionId::ReviewInboxFile,
            ActionId::ReviewInboxKeep,
            ActionId::ReviewFileTargetPrev,
            ActionId::ReviewFileTargetNext,
            ActionId::ReviewFileCommit,
            ActionId::ReviewCarryTomorrow,
            ActionId::ReviewCarryPickDate,
            ActionId::ReviewCarryLeave,
            ActionId::ReviewCarryDateCommit,
            ActionId::ReviewRecord,
            ActionId::ReviewFinish,
            ActionId::ReviewWalkPlanner,
            ActionId::ReviewWalkPrev,
            ActionId::ReviewWalkComplete,
            ActionId::ReviewAbortArm,
            ActionId::ReviewAbortAccept,
            ActionId::ReviewAbortDismiss,
            ActionId::HabitMove,
            ActionId::PickerLeft,
            ActionId::PickerRight,
            ActionId::PickerUp,
            ActionId::PickerDown,
            ActionId::PickerPrevMonth,
            ActionId::PickerNextMonth,
            ActionId::PickerMonthStart,
            ActionId::PickerMonthEnd,
            ActionId::PickerCommit,
            ActionId::MenuNext,
            ActionId::MenuPrevious,
            ActionId::MenuInvoke,
            ActionId::MenuDismiss,
            ActionId::StatusHolderKill,
            ActionId::StatusLockForceUnlock,
            ActionId::StatusDaemonRestart,
            ActionId::ExportPreview,
            ActionId::ExportClipboardCopy,
        ];
        let mut census = CENSUS.to_vec();
        census.sort();
        let mut group_less: Vec<ActionId> = REGISTRY
            .iter()
            .filter(|entry| entry.menu_group.is_none())
            .map(|entry| entry.id)
            .collect();
        group_less.sort();
        assert_eq!(
            group_less, census,
            "the group-less registry census: a new group-less \
             registration must join a group page or amend the census"
        );
        // The five lead every view chain's root page (rank order).
        let mut chains = Vec::new();
        for context in [
            ContextId::Global,
            ContextId::Today,
            ContextId::Board,
            ContextId::StackDetail,
            ContextId::TaskDetail,
            ContextId::Review,
            ContextId::Settings,
            ContextId::StatusPanel,
            ContextId::PlanNextDay,
        ] {
            chains.push(vec![context, ContextId::Global]);
            chains.push(vec![context, ContextId::ViNormal, ContextId::Global]);
        }
        for chain in &chains {
            let mut quick: Vec<(u16, usize, ActionId)> = REGISTRY
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    entry.menu_group.is_none()
                        && entry.id != ActionId::AppExit
                        && (entry.contexts.contains(&ContextId::Global)
                            || entry.contexts.iter().any(|c| chain.contains(c)))
                })
                .map(|(index, entry)| (entry.menu_rank, index, entry.id))
                .collect();
            quick.sort_by_key(|(rank, index, _)| (*rank, *index));
            let head: Vec<ActionId> = quick
                .iter()
                .map(|(_, _, id)| *id)
                .take(CURATED_FIVE.len())
                .collect();
            assert_eq!(
                head, CURATED_FIVE,
                "the curated five lead the root page over {chain:?}"
            );
        }
    }

    /// T-3 — the Task group page carries exactly the Task-group rows,
    /// enabled bits preserved, hints resolved.
    #[test]
    fn group_page_projects_the_groups_rows_with_hints() {
        let keymap = standard_keymap();
        let rows = group_page(
            &keymap,
            &board_chain(),
            &std::collections::BTreeMap::new(),
            MenuGroup::Task,
        );
        let expected = [
            ActionId::TaskCreate,
            ActionId::TaskEdit,
            // The labels manager joins the Task group over the board
            // chain (tag-color plan D-7, US-2 — rank 78, the
            // roster-only mount's opener).
            ActionId::LabelManage,
            ActionId::TaskComplete,
            // Round 2 (D19/D16): reopen and move-cancel join the Task
            // group at ranks 85/95.
            ActionId::TaskReopen,
            ActionId::TaskMove,
            ActionId::TaskMoveCancel,
            // The Today compact-move pair (US-14, #1031): the internal
            // dispatch twins join the Task group at ranks 96/97 (a row
            // dispatches the pair's own id — the one-shot mints at the
            // shared dispatch entry under a task walk landing).
            ActionId::TaskMoveUp,
            ActionId::TaskMoveDown,
            ActionId::TaskDelete,
            // UX-R8 chunk-5 D-3: the scheduled field's quick verbs
            // join at ranks 101/102/103.
            ActionId::TaskScheduleToday,
            ActionId::TaskScheduleTomorrow,
            ActionId::TaskUnschedule,
            ActionId::ReminderSnooze,
            ActionId::ReminderMute,
            ActionId::ReminderUnmute,
            ActionId::ReminderRuleAdd,
            ActionId::ReminderRuleRemove,
            // DR-3/3: the timers family joins the Task group at ranks
            // 150/160 (the registry's reviewed rows).
            ActionId::TimerCreate,
            ActionId::TimerCancel,
            // DR-5/4: the tracking arms join at ranks 170/180.
            ActionId::TaskTrackStart,
            ActionId::TaskTrackStop,
            ActionId::TaskToggleTracker,
            // The entries overlay (r4 chunk 10): the one menu row of
            // the family — rank 186, right after the toggle.
            ActionId::TaskTrackEntries,
            // The smart toggle (J3 R-8, round 9 chunk 11): rank 187,
            // between the entries row and the habit section.
            ActionId::TaskTrackToggle,
            // The habit trio (issue #506): the section-scoped verbs
            // join the Task group at ranks 200..270 (the registry's
            // reviewed rows; the arms route no commands).
            ActionId::HabitRepair,
            // The HF1 trio joins at ranks 206/207/209 (the registry's
            // reviewed rows) — between the repair and the Convert arm.
            ActionId::HabitSkip,
            ActionId::HabitPause,
            ActionId::HabitResume,
            ActionId::HabitConvertArm,
            ActionId::HabitConvertToSeries,
            // The H4 family joins at ranks 240..254 (the registry's
            // reviewed rows).
            ActionId::SeriesPause,
            ActionId::HabitEdit,
            ActionId::SeriesPauseCommit,
            // The HF3 history opener joins at rank 245 (the registry's
            // reviewed row; mounts the read-role projection dialog).
            ActionId::HabitHistory,
            ActionId::HabitRemove,
            ActionId::SeriesResume,
            ActionId::HabitRemoveConfirm,
            ActionId::SeriesConvertArm,
            ActionId::SeriesConvertToHabit,
            // The inline quota edit (HF2, W-5 #961) + its internal
            // commit verb (ranks 990/995 — section-scoped, the page's
            // tail).
            ActionId::HabitQuotaEdit,
            ActionId::HabitQuotaCommit,
        ];
        assert_eq!(
            ids(&rows),
            expected.iter().map(|id| Some(*id)).collect::<Vec<_>>()
        );
        assert!(rows.iter().all(|e| e.submenu.is_none()), "flat page");
        let complete = rows
            .iter()
            .find(|e| e.action == Some(ActionId::TaskComplete))
            .expect("row");
        assert!(complete.enabled);
        assert_eq!(complete.hint, vec!["Space".to_owned()]);
    }

    /// T-4 — the context projection over `[TaskDetail, Global]` is the
    /// Task group only, flat, no parents, no `app.exit`.
    #[test]
    fn context_entries_are_the_task_group_flat_without_quit() {
        let keymap = standard_keymap();
        let rows = context_entries(&keymap, &[ContextId::TaskDetail, ContextId::Global]);
        assert_eq!(
            ids(&rows),
            group_page(
                &keymap,
                &[ContextId::TaskDetail, ContextId::Global],
                &std::collections::BTreeMap::new(),
                MenuGroup::Task
            )
            .iter()
            .map(|e| e.action)
            .collect::<Vec<_>>()
        );
        assert!(rows.iter().all(|e| e.action != Some(ActionId::AppExit)));
        assert!(rows.iter().all(|e| e.submenu.is_none()), "no parent rows");
        // `task.complete` is Board/TaskDetail-tagged: present over the
        // task-detail chain.
        assert!(
            rows.iter()
                .any(|e| e.action == Some(ActionId::TaskComplete))
        );
    }

    /// T-5 — parent-row shape: no action, submenu set, label non-empty
    /// (structural); a group with an empty page renders no parent.
    #[test]
    fn parent_rows_carry_the_group_and_no_action() {
        let keymap = standard_keymap();
        let rows = root_page(&keymap, &board_chain(), &std::collections::BTreeMap::new());
        for parent in rows.iter().filter(|e| e.action.is_none()) {
            assert!(parent.submenu.is_some(), "a parent names its group");
            assert_ne!(parent.label, "");
            assert_ne!(parent.description, "");
            assert!(parent.hint.is_empty(), "a parent carries no binding");
        }
        // An empty group's parent is absent: the parents on the page
        // are exactly the groups whose group page is non-empty for
        // the same chain.
        let page_groups: Vec<_> = rows
            .iter()
            .filter_map(|e| e.submenu)
            .map(|sub| match sub {
                Submenu::Group(g) => g,
                Submenu::Task(_) => unreachable!("the main root page has no task families"),
            })
            .collect();
        let non_empty: Vec<_> = MenuGroup::all()
            .iter()
            .copied()
            .filter(|g| {
                !group_page(
                    &keymap,
                    &[ContextId::Global],
                    &std::collections::BTreeMap::new(),
                    *g,
                )
                .is_empty()
            })
            .collect();
        assert_eq!(page_groups, non_empty);
    }

    /// T-12 (round 2, D21) — the object context lists: each projects
    /// flat (no Quit, no parents) in its declared order, the task list
    /// equals the Task group page (T-4's equality pin, new spelling),
    /// and over `[Board, Global]` every row whose action is bound in
    /// the chain carries a non-empty hint — the F-B7 fix pin
    /// (structural: presence, never hint text).
    #[test]
    fn object_context_lists_project_flat_with_rendered_hints() {
        let keymap = standard_keymap();
        let chain = [ContextId::Board, ContextId::Global];
        for list in [TASK_CONTEXT, STACK_CONTEXT, BOARD_CONTEXT] {
            let rows = object_entries(&keymap, &chain, list, &BTreeMap::new());
            assert_eq!(rows.len(), list.len(), "one row per id");
            assert_eq!(
                ids(&rows),
                list.iter().map(|id| Some(*id)).collect::<Vec<_>>(),
                "declared order"
            );
            assert!(rows.iter().all(|e| e.action != Some(ActionId::AppExit)));
            assert!(rows.iter().all(|e| e.submenu.is_none()), "no parents");
            for entry in &rows {
                let bound = keymap
                    .first_binding_display(entry.action.expect("leaf"), &chain)
                    .is_some();
                if bound {
                    assert!(
                        !entry.hint.is_empty(),
                        "{} renders a binding hint over [Board, Global]",
                        entry.action.expect("leaf")
                    );
                }
            }
        }
        // The task list is the Task group page, spelled explicitly —
        // restricted to the card context's declared rows since the
        // tag-color D-7 widening (the labels manager joins the Task
        // *group* over the board chain — the board surface's
        // roster-only opener — but stays off the per-card context
        // list; the origin plan's W-2 names BOARD_CONTEXT only).
        let task_rows = object_entries(&keymap, &chain, TASK_CONTEXT, &BTreeMap::new());
        let task_page = group_page(
            &keymap,
            &chain,
            &std::collections::BTreeMap::new(),
            MenuGroup::Task,
        );
        let task_page_ids: Vec<_> = task_page
            .iter()
            .map(|e| e.action)
            .filter(|a| matches!(a, Some(id) if TASK_CONTEXT.contains(id)))
            .collect();
        assert_eq!(ids(&task_rows), task_page_ids);
    }

    /// T-11 — filtering rebased on the pages: a view chain offers the
    /// view's verbs but no `menu.*` rows anywhere, and nothing appears
    /// twice; a Global-only chain still gets the global rows (the
    /// menu generation stays total, §5.8).
    #[test]
    fn pages_filter_to_the_chain_and_global() {
        let keymap = standard_keymap();
        let chain = board_chain();
        let mut all: Vec<MenuEntry> =
            root_page(&keymap, &chain, &std::collections::BTreeMap::new());
        for group in MenuGroup::all() {
            all.extend(group_page(
                &keymap,
                &chain,
                &std::collections::BTreeMap::new(),
                *group,
            ));
        }
        let ids: Vec<_> = all.iter().filter_map(|e| e.action).collect();
        assert!(ids.contains(&ActionId::TaskComplete), "board verb offered");
        assert!(
            !ids.contains(&ActionId::MenuNext),
            "menu cursor never offered"
        );
        assert_eq!(
            ids.len(),
            std::collections::BTreeSet::<_>::from_iter(ids.iter()).len(),
            "nothing twice"
        );

        let global = root_page(
            &keymap,
            &[ContextId::Global],
            &std::collections::BTreeMap::new(),
        );
        assert!(global.iter().any(|e| e.action == Some(ActionId::AppExit)));
        let task = group_page(
            &keymap,
            &[ContextId::Global],
            &std::collections::BTreeMap::new(),
            MenuGroup::Task,
        );
        // The Global-tagged reminder rows survive; the view-only task
        // verbs do not.
        assert!(
            task.iter()
                .any(|e| e.action == Some(ActionId::ReminderSnooze))
        );
        assert!(
            !task
                .iter()
                .any(|e| e.action == Some(ActionId::TaskComplete))
        );
    }

    /// Ordering/hints over the pages: group-page order is rank order
    /// (registry order on ties); disabled rows stay hinted; emacs
    /// chords hint stroke-by-stroke.
    #[test]
    fn pages_are_ordered_and_disabled_rows_stay_hinted() {
        let keymap = standard_keymap();
        let rows = group_page(
            &keymap,
            &board_chain(),
            &std::collections::BTreeMap::new(),
            MenuGroup::ViewNav,
        );
        // Round 3 (D27a): the re-ranked ViewNav page — the go-to
        // verbs above the cycle, spaced 10, no ties.
        let expected = [
            ActionId::PaletteSearch, // 50
            ActionId::ViewToday,     // 60
            // The plan-next-day opener (#622): rank 65, between the
            // go-to verbs and the inbox jump.
            ActionId::ViewPlanNextDay, // 65
            ActionId::ViewInbox,       // 70
            // The review walk's opener (DR-7 chunk 3, #638): rank 75,
            // after the inbox jump, before the board focus.
            ActionId::ReviewOpen,    // 75
            ActionId::NavFocusBoard, // 80
            ActionId::NavShowTask, // 90 (palette-only availability: not chain-visible over the board chain)
            ActionId::NavNextView, // 100
            ActionId::NavPrevView, // 110
            ActionId::NavNextItem, // 120
            // The directional family's menu rows (J3 US-1, round 9
            // chunk 9): ranks 121–126, interleaved by the rank order.
            ActionId::NavFocusLeft,   // 121
            ActionId::NavFocusRight,  // 122
            ActionId::NavFocusUp,     // 123
            ActionId::NavFocusDown,   // 124
            ActionId::NavExtendNext,  // 125 (Board-only availability)
            ActionId::NavExtendPrev,  // 126 (Board-only availability)
            ActionId::NavPrevItem,    // 130
            ActionId::NavFirstItem,   // 140
            ActionId::NavLastItem,    // 150
            ActionId::NavNextControl, // 160
            ActionId::NavPrevControl, // 170
            ActionId::ViewScrollDown, // 180
            ActionId::ViewScrollUp,   // 190
        ];
        let expected: Vec<_> = expected
            .iter()
            .copied()
            // nav.show-task never renders in a menu (the Palette-only
            // availability masking).
            .filter(|id| *id != ActionId::NavShowTask)
            .collect();
        assert_eq!(
            ids(&rows),
            expected.iter().map(|id| Some(*id)).collect::<Vec<_>>()
        );
        let search = rows
            .iter()
            .find(|e| e.action == Some(ActionId::PaletteSearch))
            .expect("row");
        // The palette flipped live (chunk 9): the row reads enabled
        // and stays hinted.
        assert!(search.enabled, "the search row dispatches for real");
        assert_eq!(
            search.hint,
            vec!["Ctrl+Shift+f".to_owned()],
            "the enabled row keeps its binding hint"
        );

        let emacs = build(&RawKeymap {
            preset: Preset::Emacs,
            overrides: BTreeMap::new(),
        })
        .keymap;
        let rows = root_page(&emacs, &board_chain(), &std::collections::BTreeMap::new());
        let exit = rows
            .iter()
            .find(|e| e.action == Some(ActionId::AppExit))
            .expect("row");
        assert_eq!(
            exit.hint,
            vec!["Ctrl+x".to_owned(), "Ctrl+c".to_owned()],
            "chord hinted stroke-by-stroke"
        );
    }

    /// Hints resolve over the mode-context session chain: standard
    /// and emacs leave the mode context unbound — the walk skips it.
    #[test]
    fn pages_carry_hints_over_the_mode_context_session_chain() {
        for preset in [Preset::Standard, Preset::Emacs] {
            let keymap = build(&RawKeymap {
                preset,
                overrides: BTreeMap::new(),
            })
            .keymap;
            let rows = root_page(
                &keymap,
                &[ContextId::ViNormal, ContextId::Global],
                &std::collections::BTreeMap::new(),
            );
            let exit = rows
                .iter()
                .find(|e| e.action == Some(ActionId::AppExit))
                .expect("app.exit is global-tagged");
            assert!(
                !exit.hint.is_empty(),
                "{preset:?}: every resolvable global binding is hinted"
            );
        }
    }

    /// Round 3 (D27a): the re-rank pin — the go-to verbs ride above
    /// the next/prev cycle on the `ViewNav` page (§7.1's table).
    #[test]
    fn view_nav_page_ranks_jumps_above_the_cycle() {
        let keymap = standard_keymap();
        let rows = group_page(
            &keymap,
            &board_chain(),
            &std::collections::BTreeMap::new(),
            MenuGroup::ViewNav,
        );
        let position = |id: ActionId| {
            rows.iter()
                .position(|e| e.action == Some(id))
                .unwrap_or_else(|| panic!("{id} on the page"))
        };
        assert!(position(ActionId::ViewToday) < position(ActionId::NavNextView));
        assert!(position(ActionId::NavFocusBoard) < position(ActionId::NavNextView));
        assert!(position(ActionId::ViewInbox) < position(ActionId::NavNextView));
        assert!(position(ActionId::NavNextView) < position(ActionId::NavNextItem));
        assert!(position(ActionId::NavPrevItem) < position(ActionId::NavNextControl));
        assert!(position(ActionId::ViewScrollDown) < position(ActionId::ViewScrollUp));
        // Quick-row rank spacing is not re-checked here: the registry
        // test (`every_row_carries_a_description_and_spaced_top_ranks`)
        // iterates the same table after these rank edits and owns the
        // contract.
    }

    /// Round 3 (D27b): the Mode group renders only when the active
    /// keymap binds `mode.insert` in `ViNormal` — vi's does; standard
    /// and emacs do not. The parent row disappears with the page.
    #[test]
    fn the_mode_group_renders_only_for_vi() {
        let empty: std::collections::BTreeMap<ActionId, bool> = std::collections::BTreeMap::new();
        for preset in [Preset::Standard, Preset::Emacs] {
            let keymap = build(&RawKeymap {
                preset,
                overrides: BTreeMap::new(),
            })
            .keymap;
            let rows = root_page(&keymap, &board_chain(), &empty);
            assert!(
                !rows
                    .iter()
                    .any(|e| e.submenu == Some(Submenu::Group(MenuGroup::Mode))),
                "{preset:?}: no Mode parent row"
            );
            assert!(
                group_page(&keymap, &board_chain(), &empty, MenuGroup::Mode).is_empty(),
                "{preset:?}: the Mode page is empty"
            );
        }
        let vi = build(&RawKeymap {
            preset: Preset::Vi,
            overrides: BTreeMap::new(),
        })
        .keymap;
        let rows = root_page(&vi, &board_chain(), &empty);
        assert!(
            rows.iter()
                .any(|e| e.submenu == Some(Submenu::Group(MenuGroup::Mode))),
            "vi carries the Mode parent row"
        );
        assert!(
            !group_page(&vi, &board_chain(), &empty, MenuGroup::Mode).is_empty(),
            "vi renders the Mode page"
        );
    }

    /// Round 3 (D27c): the adapter's greying map drives the rows'
    /// enabled bits — the enumerated state rows flip per fixture
    /// state, static rows keep the registry default.
    #[test]
    fn state_rows_grey_from_the_adapter_map() {
        let keymap = standard_keymap();

        // A move flow live: cancel greys IN, everything else keeps
        // its static bit.
        let mut move_live = std::collections::BTreeMap::new();
        move_live.insert(ActionId::TaskMoveCancel, true);
        let rows = group_page(&keymap, &board_chain(), &move_live, MenuGroup::Task);
        let cancel = rows
            .iter()
            .find(|e| e.action == Some(ActionId::TaskMoveCancel))
            .expect("the cancel row");
        assert!(cancel.enabled, "the state override wins");

        // A map of falses greys the enumerated rows out (the
        // registry's `enabled` stays the default — a false override
        // is the greying itself).
        let mut greyed: std::collections::BTreeMap<ActionId, bool> =
            std::collections::BTreeMap::new();
        for id in [
            ActionId::TaskMoveCancel,
            ActionId::TaskComplete,
            ActionId::TaskReopen,
            ActionId::TaskMove,
            ActionId::TaskEdit,
            ActionId::TaskDelete,
            ActionId::TaskTitleEdit,
            ActionId::TaskTrackStart,
            ActionId::TaskTrackStop,
        ] {
            greyed.insert(id, false);
        }
        let rows = group_page(&keymap, &board_chain(), &greyed, MenuGroup::Task);
        for e in &rows {
            if greyed.contains_key(&e.action.expect("leaf")) {
                assert!(!e.enabled, "{} greys from the map", e.action.unwrap());
            } else {
                assert!(e.enabled, "{} keeps its static bit", e.action.unwrap());
            }
        }
    }

    /// UX-R8 chunk-5 D-2/D-4 (US-1/AC-4, US-2/AC-1) — the quick
    /// bar's per-subject composition as typed presence/enablement
    /// (withheld = absent from the shown set, greyed = present +
    /// disabled), never text:
    /// schedule today/tomorrow always shown; unschedule withheld on
    /// an unscheduled subject; the tracking twin's shown half flips
    /// (here → Stop, elsewhere → Start greyed, neither → Start).
    #[test]
    fn the_quick_bar_composes_per_subject() {
        let shown = |bar: &[QuickBarEntry]| -> Vec<ActionId> {
            bar.iter().filter(|e| e.visible).map(|e| e.action).collect()
        };
        // A plain, untracked, unscheduled subject: Start shown, the
        // schedule pair shown, unschedule withheld, stop withheld.
        let bar = task_quick_bar(false, false, false);
        assert_eq!(
            shown(&bar),
            vec![
                ActionId::TaskScheduleToday,
                ActionId::TaskScheduleTomorrow,
                ActionId::TaskTrackStart,
            ],
            "the plain subject's shown half"
        );
        assert!(
            bar.iter()
                .all(|e| e.action != ActionId::TaskTrackStart || e.enabled),
            "start is enabled when nothing is tracked"
        );

        // The subject holds THE open entry: stop shown + enabled,
        // start withheld.
        let bar = task_quick_bar(true, true, false);
        assert_eq!(
            shown(&bar),
            vec![
                ActionId::TaskScheduleToday,
                ActionId::TaskScheduleTomorrow,
                ActionId::TaskUnschedule,
                ActionId::TaskTrackStop,
            ],
            "the tracking subject's shown half"
        );

        // Another task's open entry: start shown GREYED (the
        // singleton precondition), stop withheld.
        let bar = task_quick_bar(false, false, true);
        let start = bar
            .iter()
            .find(|e| e.action == ActionId::TaskTrackStart)
            .expect("start shown");
        assert!(start.visible && !start.enabled, "elsewhere greys start");
    }

    /// J5 US-2 (R-3's multi-sensibility) — the multi bar's shown
    /// matrix over aggregate facts: the schedule pair always shown;
    /// unschedule rides any-scheduled; the twin only on uniform
    /// selections (uniform-tracked → stop, uniform-untracked → start
    /// greyed by elsewhere, mixed → withheld); `count == 1`
    /// degenerates to the single-card rule. No Complete/Delete icons.
    #[test]
    fn the_multi_quick_bar_composes_per_aggregate() {
        let shown = |bar: &[QuickBarEntry]| -> Vec<ActionId> {
            bar.iter().filter(|e| e.visible).map(|e| e.action).collect()
        };

        // Two untracked, unscheduled members: the schedule pair +
        // start, unschedule/stop withheld.
        let bar = multi_quick_bar(2, false, 0, false);
        assert_eq!(
            shown(&bar),
            vec![
                ActionId::TaskScheduleToday,
                ActionId::TaskScheduleTomorrow,
                ActionId::TaskTrackStart,
            ],
            "the plain selection's shown half"
        );

        // One of three scheduled: unschedule joins (the C-4 rule
        // over aggregate facts).
        let bar = multi_quick_bar(3, true, 0, false);
        assert!(
            shown(&bar).contains(&ActionId::TaskUnschedule),
            "any-scheduled shows unschedule"
        );

        // Uniformly tracked selection: stop shown, start withheld.
        let bar = multi_quick_bar(2, false, 2, false);
        assert!(shown(&bar).contains(&ActionId::TaskTrackStop));
        assert!(!shown(&bar).contains(&ActionId::TaskTrackStart));

        // Mixed tracking: the twin withheld entirely — not
        // multi-sensible.
        let bar = multi_quick_bar(3, false, 1, false);
        assert!(
            !shown(&bar).contains(&ActionId::TaskTrackStart)
                && !shown(&bar).contains(&ActionId::TaskTrackStop),
            "the mixed selection withholds the twin"
        );

        // Uniform-untracked while another task tracks: start greyed.
        let bar = multi_quick_bar(2, false, 0, true);
        let start = bar
            .iter()
            .find(|e| e.action == ActionId::TaskTrackStart)
            .expect("start shown");
        assert!(
            start.visible && !start.enabled,
            "elsewhere greys the multi start"
        );

        // `count == 1` degenerates to the single-card rule (tracked
        // here → stop; the schedule trio unchanged).
        let single = multi_quick_bar(1, false, 1, false);
        assert_eq!(
            shown(&single),
            vec![
                ActionId::TaskScheduleToday,
                ActionId::TaskScheduleTomorrow,
                ActionId::TaskTrackStop,
            ],
            "the singleton degeneration"
        );

        // The bar stays quick-verbs only: no Complete/Delete icons.
        let bar = multi_quick_bar(2, true, 2, false);
        assert!(
            bar.iter()
                .all(|e| e.action != ActionId::TaskComplete && e.action != ActionId::TaskDelete),
            "the multi bar carries no row verbs"
        );
    }

    /// UX-R8 chunk-5 D-4 (US-2/AC-1–2) — the withheld matrix, typed
    /// presence/enablement over synthesized subjects:
    /// a rule-less subject withholds Snooze/Unmute/RuleRemove while
    /// Mute stays (greyed); a series-less subject withholds the
    /// Series* rows; a tracker subject withholds Complete; the
    /// `TimerCancel` row withholds everywhere (no task-anchored timer
    /// exists — see `TaskMenuSubject::has_armed_timer`).
    #[test]
    fn the_task_context_page_withholds_inapplicable_rows() {
        let keymap = standard_keymap();
        let chain = [ContextId::Board, ContextId::Global];
        let empty = std::collections::BTreeMap::new();
        let plain = TaskMenuSubject::default();
        let (_, rows) = task_context_page(&keymap, &chain, plain, &empty, &[]);
        let ids: Vec<ActionId> = rows.iter().filter_map(|e| e.action).collect();
        assert!(
            !ids.contains(&ActionId::ReminderSnooze)
                && !ids.contains(&ActionId::ReminderUnmute)
                && !ids.contains(&ActionId::ReminderRuleRemove),
            "a rule-less subject withholds the rule-needing reminder rows"
        );
        // The family rows live behind the parents (D-5): the mute
        // verdict reads the Reminders child page.
        let reminders = task_submenu_page(&keymap, &chain, plain, &empty, TaskMenuGroup::Reminders);
        let reminder_ids: Vec<ActionId> = reminders.iter().filter_map(|e| e.action).collect();
        assert!(
            reminder_ids.contains(&ActionId::ReminderMute)
                && reminder_ids.contains(&ActionId::ReminderRuleAdd),
            "Mute and the rule-add verb stay (the applicable set)"
        );
        assert!(
            !ids.contains(&ActionId::SeriesPause),
            "a series-less subject withholds the Series* rows"
        );
        assert!(!ids.contains(&ActionId::TimerCancel), "no armed timer");
        // The tracking twin's rows carry the bar's shown half (D-4's
        // "bar + rows" verdicts): the plain subject shows Start and
        // withholds Stop; an elsewhere-tracking subject keeps the
        // greyed Start and withholds the Stop row too.
        assert!(
            ids.contains(&ActionId::TaskTrackStart) && !ids.contains(&ActionId::TaskTrackStop),
            "the plain subject's tracking rows carry the bar's shown half"
        );
        let elsewhere = TaskMenuSubject {
            tracking_elsewhere: true,
            ..TaskMenuSubject::default()
        };
        let (bar, rows) = task_context_page(&keymap, &chain, elsewhere, &empty, &[]);
        assert!(
            !rows
                .iter()
                .any(|e| e.action == Some(ActionId::TaskTrackStop)),
            "an elsewhere-tracking subject withholds the stop row"
        );
        let start = bar
            .iter()
            .find(|e| e.action == ActionId::TaskTrackStart)
            .expect("the bar shows start");
        assert!(!start.enabled, "elsewhere greys the start bar button");
        assert!(
            !bar.iter()
                .any(|e| e.action == ActionId::TaskUnschedule && e.visible),
            "the bar withholds unschedule on the unscheduled subject"
        );
        // The rule-less subject's Mute row greys (applicable verb,
        // precondition-blocked) — presence + disabled, never text.
        let mute = reminders
            .iter()
            .find(|e| e.action == Some(ActionId::ReminderMute))
            .expect("mute present");
        assert!(!mute.enabled, "mute greys without rules");

        // A scheduled, series subject flips the halves.
        let subject = TaskMenuSubject {
            scheduled: true,
            is_series: true,
            has_reminder_rules: true,
            ..TaskMenuSubject::default()
        };
        let (_, rows) = task_context_page(&keymap, &chain, subject, &empty, &[]);
        assert!(
            rows.iter()
                .any(|e| e.submenu == Some(Submenu::Task(TaskMenuGroup::Convert))),
            "the Convert family renders for a series subject"
        );
        let convert = task_submenu_page(&keymap, &chain, subject, &empty, TaskMenuGroup::Convert);
        let convert_ids: Vec<ActionId> = convert.iter().filter_map(|e| e.action).collect();
        for id in [
            ActionId::SeriesPause,
            ActionId::SeriesPauseCommit,
            ActionId::SeriesResume,
            ActionId::SeriesConvertArm,
            ActionId::SeriesConvertToHabit,
        ] {
            assert!(convert_ids.contains(&id), "{id} applies to the subject");
        }
        let reminders =
            task_submenu_page(&keymap, &chain, subject, &empty, TaskMenuGroup::Reminders);
        let reminder_ids: Vec<ActionId> = reminders.iter().filter_map(|e| e.action).collect();
        assert!(
            reminder_ids.contains(&ActionId::ReminderSnooze)
                && reminder_ids.contains(&ActionId::ReminderRuleRemove),
            "the rule-needing reminder rows apply on a rule-bearing subject"
        );

        // The #492 kind filter: a tracker subject withholds Complete
        // (the reopen no-op stays — dispatch semantics unchanged).
        let tracker = TaskMenuSubject {
            is_tracker: true,
            ..TaskMenuSubject::default()
        };
        let (_, rows) = task_context_page(&keymap, &chain, tracker, &empty, &[]);
        let ids: Vec<ActionId> = rows.iter().filter_map(|e| e.action).collect();
        assert!(!ids.contains(&ActionId::TaskComplete));
        assert!(ids.contains(&ActionId::TaskReopen));
    }

    /// UX-R8 chunk-5 D-5 (US-2/AC-3) — the submenu tree: the rare
    /// families render as parents, each child page keeping its ids
    /// and hints, and the page composes core → parents → delete last.
    #[test]
    fn the_task_context_page_composes_the_submenu_tree() {
        let keymap = standard_keymap();
        let chain = [ContextId::Board, ContextId::Global];
        let empty = std::collections::BTreeMap::new();
        let (bar, rows) =
            task_context_page(&keymap, &chain, TaskMenuSubject::default(), &empty, &[]);
        assert!(!bar.is_empty(), "the bar rides the page");
        // Core rows precede the parents; after the first parent only
        // parents and the pinned delete follow.
        let first_parent = rows
            .iter()
            .position(|e| e.action.is_none())
            .expect("parents render");
        assert!(
            rows[first_parent..]
                .iter()
                .all(|e| e.action.is_none() || e.action == Some(ActionId::TaskDelete)),
            "after the first parent only parents and the delete follow"
        );
        assert_eq!(
            rows.last().and_then(|e| e.action),
            Some(ActionId::TaskDelete),
            "the destructive row renders last (BF-W-N2)"
        );
        // The parents of the plain subject: Reminders (Mute/RuleAdd
        // apply), Timers (TimerCreate applies), Entries — Convert
        // withholds entirely (the empty-page cascade).
        let parents: Vec<_> = rows[first_parent..]
            .iter()
            .filter_map(|e| e.submenu)
            .map(|sub| match sub {
                Submenu::Task(family) => family,
                Submenu::Group(_) => unreachable!("no main-menu parents here"),
            })
            .collect();
        assert_eq!(
            parents,
            [
                TaskMenuGroup::Reminders,
                TaskMenuGroup::Timers,
                TaskMenuGroup::Entries
            ],
            "one parent per rendered family"
        );
        // Child pages keep ids + hints (the T-3 contract per family).
        let reminders = task_submenu_page(
            &keymap,
            &chain,
            TaskMenuSubject::default(),
            &empty,
            TaskMenuGroup::Reminders,
        );
        assert!(
            reminders
                .iter()
                .any(|e| e.action == Some(ActionId::ReminderRuleAdd)),
            "the family's children keep their ids"
        );
        let habit = TaskMenuSubject {
            is_habit: true,
            ..TaskMenuSubject::default()
        };
        let convert = task_submenu_page(&keymap, &chain, habit, &empty, TaskMenuGroup::Convert);
        assert!(
            convert
                .iter()
                .any(|e| e.action == Some(ActionId::HabitRepair)),
            "the habit family's children mount for a habit subject"
        );
    }

    /// Every projected row (registry rows included, parents excluded)
    /// carries a non-empty description — the tooltip source (T-6's
    /// totality, re-checked at the projection).
    #[test]
    fn projected_rows_carry_descriptions() {
        let keymap = standard_keymap();
        let chain = board_chain();
        for row in root_page(&keymap, &chain, &std::collections::BTreeMap::new())
            .iter()
            .filter(|e| e.action.is_some())
        {
            assert_ne!(row.description, "");
        }
        for group in MenuGroup::all() {
            for row in group_page(&keymap, &chain, &std::collections::BTreeMap::new(), *group) {
                assert_ne!(row.description, "");
            }
        }
    }

    /// H2 A1/D4 — the habit context page: the curated core in
    /// declared order, then the Convert parent; the task page over a
    /// `RowSubject::Task` subject is untouched (the delta pin rides
    /// the existing task tests plus the union's pure half).
    #[test]
    fn us2_s1_the_habit_context_page_projects_the_curated_core() {
        let keymap = standard_keymap();
        let chain = board_chain();
        let subject = TaskMenuSubject {
            is_habit: true,
            ..TaskMenuSubject::default()
        };
        let page = habit_context_page(&keymap, &chain, subject, &BTreeMap::new(), None);
        // The core rows in the projection's rank order, then the one
        // parent row. The arm row renders between the repair and the
        // reorder pair (the task family's shared rank space); the set
        // is pinned, the parent closes the page.
        let mut expected: Vec<Option<ActionId>> = [
            ActionId::HabitLog,
            ActionId::HabitUndoLog,
            ActionId::HabitRepair,
            // The HF1 trio rides the shared rank space between the
            // repair and the Convert arm (skip 206, pause 207,
            // resume 209 — the internal HabitPauseCommit holds 230,
            // past the whole 216 family).
            ActionId::HabitSkip,
            ActionId::HabitPause,
            ActionId::HabitResume,
            ActionId::HabitConvertArm,
            ActionId::HabitMoveUp,
            ActionId::HabitMoveDown,
            // The H4 editor opener joins the non-destructive body
            // (D1: after the reorder pair, before the Convert arm).
            ActionId::HabitEdit,
        ]
        .iter()
        .map(|&id| Some(id))
        .collect();
        // The HF3 History row rides the page body (rank 245, after
        // the editor row) ahead of the Convert parent.
        expected.push(Some(ActionId::HabitHistory));
        expected.push(None);
        // The staged remove's pair closes the page (H4 D4, the
        // destructive-last law): the arm enabled, the confirm greyed
        // while unarmed (consequence-first), no cancel step.
        expected.push(Some(ActionId::HabitRemove));
        expected.push(Some(ActionId::HabitRemoveConfirm));
        assert_eq!(
            ids(&page),
            expected,
            "declared order + parent + remove pair"
        );
        assert!(
            page.last()
                .is_some_and(|e| e.action == Some(ActionId::HabitRemoveConfirm)),
            "the remove confirm closes the page (destructive-last law)"
        );
    }

    /// H2 A3 — the armed-confirm grammar at the projection: the
    /// unarmed states map greys the confirm row and renders no cancel
    /// step; the armed map enables the confirm and joins the cancel
    /// child (the flip the adapter's re-projection drives).
    #[test]
    fn us2_s2_the_armed_states_flip_the_convert_children() {
        let keymap = standard_keymap();
        let chain = board_chain();
        let subject = TaskMenuSubject {
            is_habit: true,
            ..TaskMenuSubject::default()
        };
        let children = |states: &BTreeMap<ActionId, bool>| {
            habit_convert_children(&keymap, &chain, subject, states, TaskMenuGroup::Convert)
        };
        let unarmed = BTreeMap::from([(ActionId::HabitConvertToSeries, false)]);
        let rows = children(&unarmed);
        let confirm = rows
            .iter()
            .find(|e| e.action == Some(ActionId::HabitConvertToSeries))
            .expect("the confirm child renders");
        assert!(!confirm.enabled, "unarmed: the confirm greys");
        assert!(
            rows.iter()
                .all(|e| e.action != Some(ActionId::HabitConvertCancel)),
            "unarmed: no cancel step"
        );
        let armed = BTreeMap::from([(ActionId::HabitConvertToSeries, true)]);
        let rows = children(&armed);
        let confirm = rows
            .iter()
            .find(|e| e.action == Some(ActionId::HabitConvertToSeries))
            .expect("the confirm child renders");
        assert!(confirm.enabled, "armed: the confirm enables");
        assert!(
            rows.iter()
                .any(|e| e.action == Some(ActionId::HabitConvertCancel)),
            "armed: the cancel child joins"
        );
    }

    /// H2 D4 — the `MenuKind` spelling deviation holds: the habit menu
    /// is an anchored Object-menu projection (`MenuKind` stays
    /// `{Main, Object}`); the page is the habit one, the task page's
    /// row set for the same chain is the task one.
    #[test]
    fn the_habit_page_never_leaks_task_rows_and_vice_versa() {
        let keymap = standard_keymap();
        let chain = board_chain();
        let habit_subject = TaskMenuSubject {
            is_habit: true,
            ..TaskMenuSubject::default()
        };
        let page = habit_context_page(&keymap, &chain, habit_subject, &BTreeMap::new(), None);
        assert!(
            page.iter()
                .all(|e| e.action.is_none_or(|id| HABIT_CONTEXT.contains(&id))),
            "only the curated core (parents excluded)"
        );
        // A task subject through the task page derives no Convert
        // parent (the empty-page cascade — the union's false arm).
        let (_, task_page) = task_context_page(
            &keymap,
            &chain,
            TaskMenuSubject::default(),
            &BTreeMap::new(),
            &[],
        );
        assert!(
            task_page
                .iter()
                .all(|e| e.submenu != Some(Submenu::Task(TaskMenuGroup::Convert))),
            "a task subject renders no habit Convert parent"
        );
    }

    /// #1311 (the hints pin, projection tier) — over the habit
    /// opener's Today-prefixed chain (the derivation's one home is
    /// `open_habit_context_menu`; the journey pin covers the opener
    /// installing it), every `HABIT_CONTEXT` verb the preset binds at
    /// Today carries that exact chord in its row's `hint`, and the
    /// rows the preset deliberately leaves unbound carry no invented
    /// hint. Audit half: the equality loop over
    /// `first_binding_display` of the full chain doubles as the
    /// no-Board/Global-chord audit — no habit verb is bound outside
    /// Today, so any non-empty hint IS the Today chord.
    #[test]
    fn habit_hints_resolve_over_the_today_prefixed_chain() {
        let keymap = standard_keymap();
        let chain = vec![ContextId::Today, ContextId::Board, ContextId::Global];
        let subject = TaskMenuSubject {
            is_habit: true,
            ..TaskMenuSubject::default()
        };
        let page = habit_context_page(&keymap, &chain, subject, &BTreeMap::new(), None);
        let today_bound = [
            ActionId::HabitLog,
            ActionId::HabitSkip,
            ActionId::HabitPause,
            ActionId::HabitResume,
            ActionId::HabitUndoLog,
            ActionId::HabitRepair,
            ActionId::HabitMoveUp,
            ActionId::HabitMoveDown,
            ActionId::HabitEdit,
            ActionId::HabitHistory,
            ActionId::HabitConvertArm,
        ];
        for id in today_bound {
            let expected = keymap
                .first_binding_display(id, &chain)
                .unwrap_or_else(|| panic!("{id:?} is Today-bound in the preset"));
            let row = page
                .iter()
                .find(|e| e.action == Some(id))
                .unwrap_or_else(|| panic!("{id:?}'s row renders on the page"));
            assert_eq!(row.hint, expected, "{id:?}'s row hints its Today chord");
        }
        for row in &page {
            match row.action {
                Some(id) if today_bound.contains(&id) => {}
                // Unbound by decision (the staged remove pair, the
                // parent row, the restore trio): the pointer-first
                // posture invents no hint.
                _ => assert!(
                    row.hint.is_empty(),
                    "the unbound row invents no hint: {row:?}"
                ),
            }
        }
    }
}

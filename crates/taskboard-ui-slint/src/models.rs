// SPDX-License-Identifier: AGPL-3.0-only

//! The slint-boundary model assembly (D2/D3): builds the generated
//! window model values from presentation output and view state, and
//! maps the pure presentation enums 1:1 onto the generated slint
//! enums. The markup owns all tone/colour decisions (D18); this
//! module only ferries typed data — and the id tables (D2): the
//! indices the views return resolve against the tables built in the
//! same pass that pushed the models.

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use chrono::{DateTime, TimeDelta, Utc};

use taskboard_domain::{AppState, BoardId, ReminderRuleId, StackId, Task, TaskId};

use crate::actions::{ActionId, ContextId};
use crate::keymap::Keymap;
use crate::presentation::{self, DueKind, DueSets};
use crate::responsive::Breakpoint;
use crate::view_state::{Flow, ViewState};
use crate::{
    AppWindow, BannerRowModel, BoardBarModel, BoardModel, BoardSegmentModel,
    Breakpoint as SlintBreakpoint, DecorationKind, DueKind as SlintDueKind, FlowHintModel,
    Responsive, RuleRowModel, StackColumnModel, StackDetailModel, SyncBadgeModel, SyncPhaseUi,
    TaskCardModel, TaskDecoration, TaskDetailModel, Theme, Tone, View,
};

/// The layout cap of the banner slot (D12): the truth is the row
/// count; the model carries at most this many rows plus the overflow.
pub const BANNER_ROW_CAP: usize = 3;

/// The id tables of one refresh pass (D2): same thread, rebuilt
/// together with the models, always consistent.
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
    /// The task-detail view's reminder rules in id order (chunk 6,
    /// D12): the rule-list row indices resolve against this pass.
    pub task_rules: Vec<ReminderRuleId>,
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

/// One refresh pass (D1): reconcile the view state against `state`,
/// then push the full display model one-way onto the window and
/// return the id tables. Deterministic and timer-free — the repaint
/// timer and the tests call this.
#[must_use]
pub fn push(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    keymap: &Keymap,
    chain: &[ContextId],
    now: DateTime<Utc>,
) -> IdTables {
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
    window.set_board_bar_model(BoardBarModel {
        segments: ModelRc::new(VecModel::from(segments)),
        selected: i32::try_from(
            vs.board()
                .and_then(|b| live_boards.iter().position(|board| board.id == b))
                .unwrap_or(0),
        )
        .unwrap_or(i32::MAX),
    });

    push_chrome(window, board, keymap, chain);
    window.set_current_view(match vs.view() {
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
    push_board(window, state, vs, &sets, &stack_ids, &mut tables, now);
    push_stack_detail(window, state, vs, &sets, &mut tables, now);
    push_task_detail(window, state, vs, &sets, &mut tables, now);
    push_status_bar(window, state, vs, keymap, chain, now, &mut tables);
    tables
}

/// The toolbar chrome: board title and the `app.menu` binding hint (a
/// keymap projection, never hand-written — D19).
fn push_chrome(
    window: &AppWindow,
    board: Option<&taskboard_domain::Board>,
    keymap: &Keymap,
    chain: &[ContextId],
) {
    window.set_toolbar_title(SharedString::from(
        board.map_or(String::new(), |b| b.title.clone()),
    ));
    window.set_toolbar_menu_hint(string_model(
        &keymap
            .first_binding_display(ActionId::AppMenu, chain)
            .unwrap_or_default(),
    ));
    // The back button's hint (round-1 D12): the same projection, one
    // element class later — shown only when a binding exists.
    window.set_nav_back_hint(string_model(
        &keymap
            .first_binding_display(ActionId::NavFocusBoard, chain)
            .unwrap_or_default(),
    ));
    // The segment/card hints (round-1 D21b/D8): pushed once per
    // refresh against the visible chain.
    window.set_board_segment_hint(string_model(
        &keymap
            .first_binding_display(ActionId::BoardSelect, chain)
            .unwrap_or_default(),
    ));
    window.set_task_card_hint(string_model(
        &keymap
            .first_binding_display(ActionId::TaskEdit, chain)
            .unwrap_or_default(),
    ));
    // The stack-header hint (round-1 D18): the stack.wider binding —
    // the header tooltip's discoverability carrier.
    window.set_stack_header_hint(string_model(
        &keymap
            .first_binding_display(ActionId::StackWider, chain)
            .unwrap_or_default(),
    ));
    // The add-column tooltip's hint (r2 D6): the stack.create binding
    // — empty under emacs until chunk 3 binds it (the established
    // empty-hint rendering applies).
    window.set_stack_create_hint(string_model(
        &keymap
            .first_binding_display(ActionId::StackCreate, chain)
            .unwrap_or_default(),
    ));
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
/// kanban geometry.
fn push_board(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    stack_ids: &[StackId],
    tables: &mut IdTables,
    now: DateTime<Utc>,
) {
    let flow = vs.flow().clone();
    let columns = VecModel::default();
    for stack_id in stack_ids {
        let stack = &state.stacks[stack_id];
        let tasks = state.tasks_in_stack(*stack_id);
        let mut task_ids = Vec::with_capacity(tasks.len());
        let cards = VecModel::default();
        // The anchor card is hidden from list surfaces (DDF-5): filtered
        // once, before the loop, so cards and the id table stay in sync.
        for task in tasks {
            if presentation::is_anchor_task(task) {
                continue;
            }
            task_ids.push(task.id);
            cards.push(task_card_model(task, sets, vs, now));
        }
        tables.board_tasks.push(task_ids);
        columns.push(StackColumnModel {
            title: SharedString::from(stack.title.clone()),
            tasks: ModelRc::new(cards),
            // The create row opens in the flow's stack only (D8).
            create_open: matches!(&flow, Flow::Create { stack } if stack == stack_id),
            width_px: stack_width_px(
                vs.board(),
                *stack_id,
                vs,
                window.global::<Theme>().get_base_scale(),
            ),
        });
    }
    window.set_board_model(BoardModel {
        stacks: ModelRc::new(columns),
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
            Flow::Move { target, .. } => stack_ids
                .iter()
                .position(|s| s == target)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
            _ => -1,
        },
    });
}

/// The stack-detail model over the snapshotted subject (D7/D22).
fn push_stack_detail(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    tables: &mut IdTables,
    now: DateTime<Utc>,
) {
    let tasks = vs
        .subject_stack()
        .map(|stack| state.tasks_in_stack(stack))
        .unwrap_or_default();
    let detail_tasks = VecModel::default();
    // The same single-pass anchor filter as `push_board` (DDF-5).
    for task in &tasks {
        if presentation::is_anchor_task(task) {
            continue;
        }
        tables.stack_tasks.push(task.id);
        detail_tasks.push(task_card_model(task, sets, vs, now));
    }
    window.set_stack_model(StackDetailModel {
        title: SharedString::from(
            vs.subject_stack()
                .and_then(|s| state.stacks.get(&s))
                .map_or(String::new(), |s| s.title.clone()),
        ),
        tasks: ModelRc::new(detail_tasks),
        create_open: vs.view() == crate::view_state::ViewId::StackDetail
            && matches!(vs.flow(), Flow::Create { .. }),
    });
}

/// The task-detail model (D14): display plus the edit-flow state.
/// Rule 4 (D9): the buffer stays user-owned while the flow is active —
/// the field reads `initial-text` only on mount.
fn push_task_detail(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    tables: &mut IdTables,
    now: DateTime<Utc>,
) {
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
        rules: ModelRc::new(VecModel::from(rule_rows)),
        rule_presets: ModelRc::new(VecModel::from(
            presentation::rule_preset_labels()
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )),
        rules_hint: SharedString::from(presentation::rule_list_hint(task_rules.len())),
    });
    tables.task_rules = task_rules;
}

/// The status bar: sync badge, the capped banner rows, the flow hint
/// (D11/D12).
fn push_status_bar(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    keymap: &Keymap,
    chain: &[ContextId],
    now: DateTime<Utc>,
    tables: &mut IdTables,
) {
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
    let rows = VecModel::default();
    for row in banner.iter().take(BANNER_ROW_CAP) {
        rows.push(BannerRowModel {
            title: SharedString::from(
                state
                    .tasks
                    .get(&row.task)
                    .map_or(String::new(), |t| t.title.clone()),
            ),
            due_phrase: SharedString::from(state.tasks.get(&row.task).map_or(String::new(), |t| {
                presentation::due_phrase(DueKind::Overdue, t.duedate, now)
            })),
        });
    }
    window.set_banner_model(ModelRc::new(rows));
    window.set_banner_overflow(i32::try_from(overflow).unwrap_or(i32::MAX));

    let flow_hint_keys = match vs.flow() {
        Flow::Move { .. } => keymap
            .first_binding_display(ActionId::TaskMove, chain)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    window.set_flow_hint_model(FlowHintModel {
        keys: string_model(&flow_hint_keys),
        visible: matches!(vs.flow(), Flow::Move { .. }),
    });
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
        decorations: ModelRc::new(VecModel::from(field_decorations(task))),
    }
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
}

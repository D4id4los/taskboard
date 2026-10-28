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
    /// `Some(group)` for a submenu-parent row (round-1 D6).
    pub submenu: Option<MenuGroup>,
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

fn row(keymap: &Keymap, chain: &[ContextId], entry: &crate::actions::ActionEntry) -> MenuEntry {
    MenuEntry {
        action: Some(entry.id),
        label: entry.label,
        description: entry.description,
        enabled: entry.enabled,
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
pub fn root_page(keymap: &Keymap, chain: &[ContextId]) -> Vec<MenuEntry> {
    let visible = chain_visible(chain);
    let mut quick: Vec<(u16, usize, MenuEntry)> = visible
        .iter()
        .filter(|(_, entry)| entry.menu_group.is_none() && entry.id != ActionId::AppExit)
        .map(|(index, entry)| (entry.menu_rank, *index, row(keymap, chain, entry)))
        .collect();
    quick.sort_by_key(|(rank, index, _)| (*rank, *index));

    let mut page: Vec<MenuEntry> = quick.into_iter().map(|(_, _, entry)| entry).collect();
    for group in MenuGroup::all() {
        if !group_page(keymap, chain, *group).is_empty() {
            page.push(MenuEntry {
                action: None,
                label: group.label(),
                description: "Opens this group of actions.",
                enabled: true,
                hint: Vec::new(),
                submenu: Some(*group),
            });
        }
    }
    if let Some((_, exit)) = visible
        .iter()
        .find(|(_, entry)| entry.id == ActionId::AppExit)
    {
        page.push(row(keymap, chain, exit));
    }
    page
}

/// One group's page (round-1 D6): the group's chain-visible rows in
/// `(menu_rank, registry order)`.
#[must_use]
pub fn group_page(keymap: &Keymap, chain: &[ContextId], group: MenuGroup) -> Vec<MenuEntry> {
    let mut rows: Vec<(u16, usize, MenuEntry)> = chain_visible(chain)
        .iter()
        .filter(|(_, entry)| entry.menu_group == Some(group))
        .map(|(index, entry)| (entry.menu_rank, *index, row(keymap, chain, entry)))
        .collect();
    rows.sort_by_key(|(rank, index, _)| (*rank, *index));
    rows.into_iter().map(|(_, _, entry)| entry).collect()
}

/// The task-card context menu (round-1 D13, round-2 D21 spelling):
/// the Task group's chain-visible rows, flat (no parents), no Quit
/// pin. The reopen and move-cancel rows join automatically with their
/// Task-group registry rows.
#[must_use]
pub fn context_entries(keymap: &Keymap, chain: &[ContextId]) -> Vec<MenuEntry> {
    group_page(keymap, chain, MenuGroup::Task)
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
    ActionId::ReminderSnooze,
    ActionId::ReminderMute,
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
    ActionId::BoardDelete,
];

/// One object context menu (round-2 D21): the named actions
/// projected through the same `row()` as every page — binding hints
/// included. Over `[Board, Global]` (the one chain every object menu
/// opens with) every board-surface verb's binding is visible, so the
/// F-B7 empty-hint bug cannot recur. No static enablement gating:
/// gating is dispatch-time (`NoTarget`/inert), the established
/// `task.complete`-on-no-focus machinery.
#[must_use]
pub fn object_entries(keymap: &Keymap, chain: &[ContextId], ids: &[ActionId]) -> Vec<MenuEntry> {
    ids.iter()
        .filter_map(|id| REGISTRY.iter().find(|entry| entry.id == *id))
        .map(|entry| row(keymap, chain, entry))
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

    /// T-1 — the curated root page: the four quick rows in rank
    /// order, then the parent rows in group order (every group's page
    /// is non-empty over the board chain; six since the round-2
    /// Stack/Board split), then Quit last.
    #[test]
    fn root_page_is_curated_quick_rows_parents_quit() {
        let keymap = standard_keymap();
        let rows = root_page(&keymap, &board_chain());
        let mut actions = rows.iter().filter_map(|e| e.action);
        for id in [
            ActionId::AppSettings,
            ActionId::CaptureQuick,
            ActionId::AppUndo,
            ActionId::AppRedo,
        ] {
            assert_eq!(actions.next(), Some(id), "quick row order");
        }
        // Then the parent rows (no action), then Quit.
        let parents: Vec<_> = rows
            .iter()
            .filter(|e| e.action.is_none())
            .map(|e| e.submenu)
            .collect();
        assert_eq!(
            parents,
            MenuGroup::all()
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>(),
            "one parent per group, in group order"
        );
        assert_eq!(
            *rows.last().expect("non-empty"),
            MenuEntry {
                action: Some(ActionId::AppExit),
                label: "Quit taskboard",
                description: "Quits the application; local state is already saved.",
                enabled: true,
                hint: vec!["Ctrl+Q".to_owned()],
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
            let rows = root_page(&keymap, &chain);
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

    /// T-3 — the Task group page carries exactly the Task-group rows,
    /// enabled bits preserved, hints resolved.
    #[test]
    fn group_page_projects_the_groups_rows_with_hints() {
        let keymap = standard_keymap();
        let rows = group_page(&keymap, &board_chain(), MenuGroup::Task);
        let expected = [
            ActionId::TaskCreate,
            ActionId::TaskEdit,
            ActionId::TaskComplete,
            // Round 2 (D19/D16): reopen and move-cancel join the Task
            // group at ranks 85/95.
            ActionId::TaskReopen,
            ActionId::TaskMove,
            ActionId::TaskMoveCancel,
            ActionId::TaskDelete,
            ActionId::ReminderSnooze,
            ActionId::ReminderMute,
            ActionId::ReminderRuleAdd,
            ActionId::ReminderRuleRemove,
            // DR-3/3: the timers family joins the Task group at ranks
            // 150/160 (the registry's reviewed rows).
            ActionId::TimerCreate,
            ActionId::TimerCancel,
            // DR-5/4: the tracking arms join at ranks 170/180.
            ActionId::TaskTrackStart,
            ActionId::TaskTrackStop,
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
        let rows = root_page(&keymap, &board_chain());
        for parent in rows.iter().filter(|e| e.action.is_none()) {
            assert!(parent.submenu.is_some(), "a parent names its group");
            assert_ne!(parent.label, "");
            assert_ne!(parent.description, "");
            assert!(parent.hint.is_empty(), "a parent carries no binding");
        }
        // An empty group's parent is absent: the parents on the page
        // are exactly the groups whose group page is non-empty for
        // the same chain.
        let page_groups: Vec<_> = rows.iter().filter_map(|e| e.submenu).collect();
        let non_empty: Vec<_> = MenuGroup::all()
            .iter()
            .copied()
            .filter(|g| !group_page(&keymap, &[ContextId::Global], *g).is_empty())
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
            let rows = object_entries(&keymap, &chain, list);
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
        // The task list is the Task group page, spelled explicitly.
        let task_rows = object_entries(&keymap, &chain, TASK_CONTEXT);
        let task_page = group_page(&keymap, &chain, MenuGroup::Task);
        assert_eq!(
            ids(&task_rows),
            task_page.iter().map(|e| e.action).collect::<Vec<_>>()
        );
    }

    /// T-11 — filtering rebased on the pages: a view chain offers the
    /// view's verbs but no `menu.*` rows anywhere, and nothing appears
    /// twice; a Global-only chain still gets the global rows (the
    /// menu generation stays total, §5.8).
    #[test]
    fn pages_filter_to_the_chain_and_global() {
        let keymap = standard_keymap();
        let chain = board_chain();
        let mut all: Vec<MenuEntry> = root_page(&keymap, &chain);
        for group in MenuGroup::all() {
            all.extend(group_page(&keymap, &chain, *group));
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

        let global = root_page(&keymap, &[ContextId::Global]);
        assert!(global.iter().any(|e| e.action == Some(ActionId::AppExit)));
        let task = group_page(&keymap, &[ContextId::Global], MenuGroup::Task);
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
        let rows = group_page(&keymap, &board_chain(), MenuGroup::ViewNav);
        let expected = [
            ActionId::PaletteSearch,
            ActionId::NavNextView,
            ActionId::NavPrevView,
            ActionId::NavFocusBoard,
            // Chunk 10 (D3): view.today joins at rank 90, view.inbox
            // at 100 — rank ties keep registry order.
            ActionId::ViewToday,
            ActionId::NavNextItem,
            ActionId::ViewInbox,
            ActionId::NavPrevItem,
            // Round 2 (D20): the first/last jumps join the ViewNav
            // page at ranks 105/115.
            ActionId::NavFirstItem,
            ActionId::NavLastItem,
        ];
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
            vec!["Ctrl+F".to_owned()],
            "the enabled row keeps its binding hint"
        );

        let emacs = build(&RawKeymap {
            preset: Preset::Emacs,
            overrides: BTreeMap::new(),
        })
        .keymap;
        let rows = root_page(&emacs, &board_chain());
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
            let rows = root_page(&keymap, &[ContextId::ViNormal, ContextId::Global]);
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

    /// Every projected row (registry rows included, parents excluded)
    /// carries a non-empty description — the tooltip source (T-6's
    /// totality, re-checked at the projection).
    #[test]
    fn projected_rows_carry_descriptions() {
        let keymap = standard_keymap();
        let chain = board_chain();
        for row in root_page(&keymap, &chain)
            .iter()
            .filter(|e| e.action.is_some())
        {
            assert_ne!(row.description, "");
        }
        for group in MenuGroup::all() {
            for row in group_page(&keymap, &chain, *group) {
                assert_ne!(row.description, "");
            }
        }
    }
}

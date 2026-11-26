// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(test)]
mod effort_parity_tests {
    // AC-3 (task-detail-editor UX plan US-3/D-8): the detail row and
    // the walk's placement parse (`commit_plan_place`) share ONE
    // predicate — the domain `Effort::parse_text_form`. The corpus
    // pins the agreement: both fronts accept/reject identically
    // because both call sites route the same function; the journeys
    // us3_s1/us3_s2 carry the funnel's behavioral proof.
    use crate::adapter::parse_effort_row;

    #[test]
    fn us3_s3_the_effort_row_parse_agrees_with_the_walk_parse() {
        for (text, accepted) in [
            ("45", true),
            ("1h30m", true),
            ("2h", true),
            ("90s", true),
            ("0", true),
            ("junk", false),
            ("1x", false),
            ("-5", false),
            ("h", false),
        ] {
            assert_eq!(
                parse_effort_row(text).is_some(),
                accepted,
                "{text}: the row's parse must be the walk's parse"
            );
            assert_eq!(
                parse_effort_row(text).is_some(),
                taskboard_domain::Effort::parse_text_form(text).is_some(),
                "{text}: one predicate, two fronts"
            );
        }
    }
}

#[cfg(test)]
mod menu_label_projection_tests {
    use crate::adapter::*;
    use crate::menu::MenuEntry;

    /// The #1283 pin: an owned (engine-derived) label projects through
    /// the shell model verbatim — structural equality on the
    /// projected value, not text-in-DOM. The borrowed leg keeps
    /// projecting too (the source-compatibility half).
    #[test]
    fn an_owned_label_projects_through_the_shell_model() {
        let rows = vec![
            MenuEntry {
                action: Some(ActionId::HabitLog),
                label: std::borrow::Cow::Owned("Restore habit \u{201c}stretch\u{201d}".to_owned()),
                description: "engine-derived",
                enabled: true,
                hint: Vec::new(),
                submenu: None,
            },
            MenuEntry {
                action: Some(ActionId::AppExit),
                label: std::borrow::Cow::Borrowed("Quit taskboard"),
                description: "registry",
                enabled: true,
                hint: Vec::new(),
                submenu: None,
            },
        ];
        let model = Adapter::menu_model(&rows);
        assert_eq!(
            model.row_data(0).expect("row 0").label.as_str(),
            "Restore habit \u{201c}stretch\u{201d}",
            "the owned label survives the bridge verbatim"
        );
        assert_eq!(
            model.row_data(1).expect("row 1").label.as_str(),
            "Quit taskboard",
            "the borrowed label projects unchanged"
        );
    }
}

/// The round-8 B4 small-inventory seam pins (#1429, #1478, #1328,
/// #1449): typed seam-level outcomes at the lowest layer — the
/// adapter's effect/seam tier — never text oracles. A child module of
/// `adapter` so the #1478 direct construction can reach
/// `route_menu_action` (the arm-time dead subject is not reachable
/// through today's live-only menu openers).
#[cfg(test)]
mod ui_small_inventory_tests {
    use crate::KeyboardState;
    use crate::actions::ActionId;
    use crate::adapter::*;
    use crate::router::UiEffect;
    use crate::test_platform::on_slint_thread;
    use crate::{UiHandle, UiSettings, mount};
    use std::sync::Arc;
    use taskboard_domain::{
        AppState, Board, BoardClocks, Color, HabitClocks, HabitId, HabitTarget, NonZeroCount,
        Stack, StackClocks, StackId, Task, TaskClocks, TaskId,
    };
    use tokio::sync::{broadcast, mpsc};

    // -- fixture -------------------------------------------------------------

    fn fixture_board(id: taskboard_domain::BoardId) -> Board {
        let now = BoardClocks::epoch();
        Board {
            id,
            remote: None,
            title: "fixture board".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: now,
                color: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        }
    }

    fn fixture_habit(board: taskboard_domain::BoardId) -> taskboard_domain::habit::Habit {
        taskboard_domain::habit::Habit {
            id: HabitId::from_uuid(uuid::Uuid::now_v7()),
            board,
            title: "water".to_owned(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(2).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(chrono::Utc::now()),
            continued_from: None,
            deleted: false,
            by: None,
            paused_until: None,
        }
    }

    /// One board, one stack, one titled task, one live habit — the
    /// smallest state every pin in this file can read.
    fn fixture() -> (AppState, TaskId, HabitId) {
        let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
        let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
        let task_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let now = BoardClocks::epoch();
        let mut state = AppState::default();
        state.boards.insert(board_id, fixture_board(board_id));
        let stack = Stack {
            id: stack_id,
            remote: None,
            board: board_id,
            title: "stack".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: now,
                order: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        };
        state.stacks.insert(stack_id, stack);
        let task = Task {
            id: task_id,
            remote: None,
            label_memberships: BTreeMap::new(),
            title: "fixture task".to_owned(),
            description: String::new(),
            duedate: Some(chrono::Utc::now() - chrono::TimeDelta::try_hours(2).unwrap()),
            scheduled: None,
            done: None,
            stack: stack_id,
            order: 0,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: now,
                description: now,
                duedate: now,
                scheduled: now,
                done: now,
                position: now,
                effort: now,
                importance: now,
                urgency: now,
                recur: now,
                kind: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        };
        state.tasks.insert(task_id, task);
        let habit = fixture_habit(board_id);
        let habit_id = habit.id;
        state.habits.habits.insert(habit_id, habit);
        (state, task_id, habit_id)
    }

    // -- rig -----------------------------------------------------------------

    struct Rig {
        handle: UiHandle,
        _commands_rx: mpsc::Receiver<taskboard_domain::StateCommand>,
    }

    fn rig(state: AppState) -> Rig {
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_system_tx, system_rx) = broadcast::channel(16);
        let (_signals_tx, signals_rx) = broadcast::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            Arc::new(arc_swap::ArcSwap::from_pointee(state)),
            signals_rx,
            system_rx,
            commands_tx,
            requests_tx,
            UiSettings::default(),
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        Rig {
            handle,
            _commands_rx: commands_rx,
        }
    }

    impl Rig {
        fn adapter(&self) -> std::rc::Rc<std::cell::RefCell<crate::adapter::Adapter>> {
            self.handle.test_adapter()
        }

        /// Routes one action through the adapter's dispatch seam (the
        /// src tests' `route_action` pattern).
        fn route(&self, action: ActionId) {
            self.adapter()
                .borrow_mut()
                .route(self.handle.window(), action);
        }
    }

    // -- #1429: the right-click seam seats the focus-first discipline -------

    /// The habit row's right-click is the row activation's twin: the walk
    /// cursor seats (`focused_habit` mints, the exclusivity law clears a
    /// prior task landing) and the menu opens only past the landed focus.
    #[test]
    fn habit_context_seats_the_focus_and_opens_the_menu() {
        on_slint_thread(|| {
            let (state, _task, habit) = fixture();
            let rig = rig(state);
            // The exclusivity precondition: a task landing holds the walk
            // (the walk's first row on Today is the fixture task — tasks
            // lead, habits follow).
            rig.route(ActionId::NavNextItem);
            assert!(
                rig.adapter().borrow().view_state.focused_task().is_some(),
                "precondition: the task landing holds the cursor"
            );

            rig.handle
                .window()
                .invoke_habits_context_requested(0, 120.0, 120.0);

            let adapter = rig.adapter();
            let a = adapter.borrow();
            assert_eq!(
                a.test_focused_habit(),
                Some(habit),
                "the right-click mints the habit focus fact"
            );
            assert!(
                a.view_state.focused_task().is_none(),
                "the exclusivity law clears the task cursor (#1429)"
            );
            assert!(
                rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the menu opens past the landed focus"
            );
        });
    }

    /// A stale right-click index is absorbed: no menu, no panic, no focus
    /// mint (the notification-row guard shape the left-click seam uses).
    #[test]
    fn habit_context_at_a_stale_index_absorbs_silently() {
        on_slint_thread(|| {
            let (state, _task, _habit) = fixture();
            let rig = rig(state);
            rig.handle
                .window()
                .invoke_habits_context_requested(9, 120.0, 120.0);
            let adapter = rig.adapter();
            let a = adapter.borrow();
            assert_eq!(
                a.test_focused_habit(),
                None,
                "a stale index never mints the focus fact"
            );
            assert!(
                !rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "no focus landed, no menu opens"
            );
        });
    }

    // -- #1478: the refused quota route disarms the restored focus ----------

    /// The quota menu route over an arm-time dead subject refuses (the
    /// typed notice lane), and the route's dispatch-scoped disarm now
    /// covers the survivor: the restored pre-close `focused_habit` dies
    /// with the refusal instead of outrunning it. The refusal path is
    /// constructed directly at the `route_menu_action` tier (the arm-time
    /// dead subject — no walkthrough through today's live-only openers can
    /// produce it).
    #[test]
    fn a_refused_quota_route_clears_the_restored_focus() {
        on_slint_thread(|| {
            let (state, _task, habit) = fixture();
            let rig = rig(state);
            // The row click seats the focus fact (the menu routes restore
            // exactly this capture).
            rig.handle.window().invoke_habits_row_activated(0);
            assert_eq!(
                rig.adapter().borrow().test_focused_habit(),
                Some(habit),
                "precondition: the row click seated the focus fact"
            );

            // The subject dies before the quota route (the arm-time dead
            // subject).
            let mut dead = rig.handle.test_state().load_full().as_ref().clone();
            let row = dead.habits.habits.get_mut(&habit).expect("fixture row");
            row.deleted = true;
            rig.handle.test_state().store(Arc::new(dead));

            // The quota menu route: the close leg restores the captured
            // focus, then the dispatch refuses on the dead subject.
            {
                let adapter_rc = rig.adapter();
                let mut a = adapter_rc.borrow_mut();
                a.route_menu_action(rig.handle.window(), ActionId::HabitQuotaEdit);
            }

            let adapter = rig.adapter();
            let a = adapter.borrow();
            assert!(
                !rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the route closes the menu"
            );
            assert!(a.test_notice_armed(), "the refusal surfaces typed");
            assert_eq!(
                a.test_focused_habit(),
                None,
                "the refusal disarms the restored focus (#1478)"
            );
        });
    }

    // -- #1328: the MenuBack root-close leg honours the reset law -----------

    /// The Escape ladder's final close resets the page models exactly like
    /// `apply_close_menu` — `menu_entries`/`root_entries`/`menu_chain`/
    /// `menu_kind`/`menu_subject`/`quick_bar_entries` never survive the
    /// close.
    #[test]
    fn the_menu_back_root_close_resets_the_page_models() {
        on_slint_thread(|| {
            let (state, _task, _habit) = fixture();
            let rig = rig(state);
            // The main menu open (the root page's chain and entries mount).
            rig.route(ActionId::AppMenu);
            assert!(
                rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the menu opened"
            );
            assert!(
                !rig.adapter().borrow().test_menu_models_reset(),
                "precondition: an open menu holds page models"
            );

            // The ladder's close leg: MenuBack at the root.
            rig.adapter()
                .borrow_mut()
                .apply_effect_for_test(rig.handle.window(), UiEffect::MenuBack);

            assert!(
                !rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the close closed"
            );
            assert!(
                rig.adapter().borrow().test_menu_models_reset(),
                "the close leg resets every page model (#1328)"
            );
        });
    }

    // -- #1315: the habit card's hint-keys ride the keymap projection -------

    /// The habit row card's tooltip hint-keys equal the keymap's
    /// first-binding projection for the row's edit verb over the visible
    /// chain — the task-card grammar's structural oracle (the rendered
    /// model is compared against the projection's own output, never label
    /// prose). Red pre-fix: the card model hardcoded the empty list.
    #[test]
    fn the_habit_card_hint_keys_match_the_keymap_projection() {
        on_slint_thread(|| {
            use slint::Model as _;
            let (state, _task, _habit) = fixture();
            let rig = rig(state);
            let chain = rig.adapter().borrow().shell.chain();
            let keymap = crate::keymap::Keymap::from_preset(crate::keymap::Preset::Standard);
            let projected: Vec<slint::SharedString> = keymap
                .first_binding_display(ActionId::HabitEdit, &chain)
                .unwrap_or_default()
                .iter()
                .map(|stroke| slint::SharedString::from(stroke.as_str()))
                .collect();

            let model = rig.handle.window().get_habits_model();
            let row = model.row_data(0).expect("the fixture habit row renders");
            let rendered: Vec<slint::SharedString> = (0..row.hint_keys.row_count())
                .filter_map(|i| row.hint_keys.row_data(i))
                .collect();
            assert_eq!(
                rendered, projected,
                "#1315: the habit card's hint-keys are the keymap projection"
            );
        });
    }

    // -- #1449: the task variant titles the open task -----------------------

    /// The `TaskDetail` task variant's toolbar title is the open task's
    /// STORED title — the habit arm's shape one arm over; the board's
    /// title never answers for the detail root (the fixture renames the
    /// board so the two oracles cannot collide).
    #[test]
    fn the_task_variant_titles_the_open_task() {
        on_slint_thread(|| {
            let (state, task, _habit) = fixture();
            let rig = rig(state);
            // The detail mounts over the fixture task (the walk lands, the
            // activation opens the detail root).
            rig.route(ActionId::NavNextItem);
            rig.route(ActionId::TaskEdit);
            assert_eq!(
                rig.handle.window().get_current_view(),
                crate::View::TaskDetail,
                "precondition: the task detail root mounts"
            );
            let stored = rig
                .handle
                .test_state()
                .load()
                .tasks
                .get(&task)
                .map(|t| t.title.clone())
                .expect("fixture task");
            assert_eq!(
                rig.handle.window().get_toolbar_title(),
                stored.as_str(),
                "the toolbar title is the open task's stored title (#1449)"
            );
        });
    }
}

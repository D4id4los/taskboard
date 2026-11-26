// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, SharedString};

use taskboard_domain::TaskId;

use crate::AppWindow;
use crate::actions::ActionId;

use super::Adapter;
use super::{send_card_track_toggle, send_toggled_done, settle_pointer_conflict};
use crate::adapter::install_seam;

/// The today view's row seams (chunk 10 D7): click, activation, and
/// context — every one focuses the row first (the board card's §7.1
/// discipline, with the selection-follows-focus retarget), then the
/// activation/context seams route through the action layer's verbs.
#[allow(clippy::too_many_lines)] // one seam block, kept flat
pub(in crate::adapter) fn install_today_row_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_today_row_clicked, adapter: adapter;
    bind mut (a, window), args (section, row), {        settle_pointer_conflict(&mut a, &window);
    focus_today_row(&mut a, &window, section, row);

    });
    // -- the plan-next-day surface (#622, D-3/D-6) --
    install_seam!(window: window.on_plan_box_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.begin_plan_place(&window, task, true);

    });

    install_seam!(window: window.on_plan_pool_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(
            a.tables
                .plan_box_count
                .saturating_add(usize::try_from(index).unwrap_or(usize::MAX)),
        )
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.begin_plan_place(&window, task, false);

    });

    install_seam!(window: window.on_plan_box_earlier, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanBoxEarlier);

    });

    install_seam!(window: window.on_plan_box_later, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanBoxLater);

    });

    install_seam!(window: window.on_plan_box_remove, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanRemoveBox);

    });

    install_seam!(window: window.on_plan_time_edited, adapter: adapter;
    bind mut (a, _window), args (text), {        // The D9 mirror rule: the level-held buffers update per
    // keystroke; a refresh never clobbers them while the flow
    // holds them.
    if let Some(place) = a.view_state.plan_place_mut() {
        place.time_text = text.to_string();
    }

    });

    install_seam!(window: window.on_plan_effort_edited, adapter: adapter;
    bind mut (a, _window), args (text), {        if let Some(place) = a.view_state.plan_place_mut() {
        place.effort_text = text.to_string();
    }

    });

    install_seam!(window: window.on_plan_place_commit, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::PlanCommitPlace);
    if matches!(a.view_state.flow(), crate::view_state::Flow::PlanPlace { .. }) {
        // The parse refusal kept the form: echo the typed text back
        // (the due editor's validity flavour, minus the tint).
        window.set_plan_time_text(SharedString::from(
            a.view_state
                .plan_place()
                .map_or(String::new(), |place| place.time_text.clone()),
        ));
        window.set_plan_effort_text(SharedString::from(
            a.view_state
                .plan_place()
                .map_or(String::new(), |place| place.effort_text.clone()),
        ));
    } else {
        window.set_plan_time_text(SharedString::new());
        window.set_plan_effort_text(SharedString::new());
    }

    });

    install_seam!(window: window.on_plan_place_cancel, adapter: adapter;
    bind mut (a, _window), args (), {        // The cancel's wipe is the level's drop (D-H2).
    a.view_state.close_plan_place();

    });

    // #592: the live rows' extent reports feed the height cache
    // (deduped — a no-op report costs one map insert comparison).
    // #1310/#1304 (2026-11-24): a report that CHANGES a cached
    // extent also re-derives — the today sections' clip heights bind
    // to the cache-summed content extents (`push_today`), so a
    // first-materializing row's report must land as a model update,
    // not sit for the next unrelated pass. The delegate-side
    // last-report gate bounds the loop: only a real height change
    // reports, and a re-derivation never changes row widths, so no
    // further report fires (the refresh settles in one pass).
    install_seam!(window: window.on_today_row_extent, adapter: adapter;
    bind mut (a, window), args (id, extent), {
        if a.models.today_row_extents.get(&id) != Some(&extent) {
            a.models.today_row_extents.insert(id.clone(), extent);
            // The section's content extent updates SYNCHRONOUSLY
            // (round-6 A1, #1310/#1304): the clip height binding
            // reads the `TodayClipFacts` extents, so a one-pass lag
            // here would leave the clip smaller than its rows for a
            // pass — the virtualizer's placement math scrolls the
            // content by exactly that difference. The incremental
            // sum matches `push_today`'s wholesale re-derivation.
            if let Some(section) = a
                .models
                .today_section_row_ids
                .iter()
                .position(|ids| ids.iter().any(|row_id| row_id == &id))
            {
                let sum: f32 = a.models.today_section_row_ids[section]
                    .iter()
                    .map(|row_id| {
                        a.models
                            .today_row_extents
                            .get(row_id)
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .sum();
                a.models.today_section_extents[section] = sum;
                let facts = window.global::<crate::TodayClipFacts>();
                match section {
                    0 => facts.set_e0(sum),
                    1 => facts.set_e1(sum),
                    2 => facts.set_e2(sum),
                    _ => facts.set_e3(sum),
                }
            }
        }
    });

    // #1310/#1304 (2026-11-24): the section viewports' realized
    // geometry report (the `section-clip-geometry` witness binding)
    // mirrored into the `TodayClipFacts` display global — the tests'
    // #1023 root seam. Display state only; the report is
    // re-evaluated declaratively by the view, never a command, so no
    // refresh rides it (the raw handler, not `install_seam!`: the
    // callback returns the witness's length).
    {
        let seam_weak = window.as_weak();
        window.global::<crate::TodayClipFacts>().on_section_height(
            move |section: i32, extent: f32, cap: f32| {
                let Some(window) = seam_weak.upgrade() else {
                    return 0.0;
                };
                let facts = window.global::<crate::TodayClipFacts>();
                let height = extent.min(cap);
                match section {
                    0 => facts.set_h0(height),
                    1 => facts.set_h1(height),
                    2 => facts.set_h2(height),
                    _ => facts.set_h3(height),
                }
                height
            },
        );
    }

    install_seam!(window: window.on_today_row_activated, adapter: adapter;
    bind mut (a, window), args (section, row), {        settle_pointer_conflict(&mut a, &window);
    if focus_today_row(&mut a, &window, section, row).is_some() {
        a.route(&window, ActionId::TaskEdit);
    }

    });
    // The done checkbox (r3 D6): the toggled command, the board
    // cards' seam verbatim (a stale index absorbed).
    install_seam!(window: window.on_today_row_done_toggled, adapter: adapter; [bare_ref]
    bind (a), args (section, row), {        let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
        return;
    };
    if let Some(task) = a.tables.today_row(section, row) {
        send_toggled_done(&a, task);
    }

    });

    // The card play/pause control (UX-R8 chunk-5 D-6): the today
    // leg's addressing chain.
    install_seam!(window: window.on_today_row_track_toggled, adapter: adapter;
    bind mut (a, window), args (section, row), {        let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
        return;
    };
    if let Some(task) = a.tables.today_row(section, row) {
        send_card_track_toggle(&mut a, &window, task);
    }

    });
    // The Tracked section's activation (#490, D-TS7): resolves the
    // row through the tracked-today table and opens the task — the
    // today-row-activated arm verbatim (click ≡ key, §7.1).
    install_seam!(window: window.on_today_tracked_activated, adapter: adapter;
    bind mut (a, window), args (row), {        settle_pointer_conflict(&mut a, &window);
    if focus_tracked_today_row(&mut a, &window, row).is_some() {
        a.route(&window, ActionId::TaskEdit);
    }

    });
    install_seam!(window: window.on_today_row_context_requested, adapter: adapter;
    bind mut (a, window), args (section, row, x, y), {        settle_pointer_conflict(&mut a, &window);
    if focus_today_row(&mut a, &window, section, row).is_some() {
        // The today surface carries no selection (R-1's scoping).
        a.open_task_context_menu(&window, x, y, &[ActionId::TaskDueToday], None);
    }

    });
}

/// A today row at `(section, row)`: resolves the id tables (the
/// per-section prefix sums over `today_tasks`) and applies the click's
/// focus state. Returns the addressed task, or `None` for a stale
/// index against an older tables pass.
pub(in crate::adapter) fn focus_tracked_today_row(
    adapter: &mut Adapter,
    window: &AppWindow,
    row: i32,
) -> Option<TaskId> {
    let Ok(row) = usize::try_from(row) else {
        return None;
    };
    let task = adapter.tables.tracked_today.get(row).copied()?;
    let stack = adapter.state.load().tasks.get(&task).map(|t| t.stack);
    if let Some(stack) = stack {
        adapter.view_state.activate_card(task, stack);
    } else {
        adapter.view_state.set_focused(Some(task));
    }
    adapter.finish_mutation(window);
    Some(task)
}

pub(in crate::adapter) fn focus_today_row(
    adapter: &mut Adapter,
    window: &AppWindow,
    section: i32,
    row: i32,
) -> Option<TaskId> {
    let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
        return None;
    };
    let task = adapter.tables.today_row(section, row)?;
    let stack = {
        let state = adapter.state.load();
        state.tasks.get(&task).map(|t| t.stack)?
    };
    let outcomes = adapter
        .view_state
        .activate_today_row(&adapter.state.load(), task, stack);
    adapter.apply_outcomes(window, &outcomes);
    Some(task)
}

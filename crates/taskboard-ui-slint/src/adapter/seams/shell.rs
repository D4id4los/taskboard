// SPDX-License-Identifier: AGPL-3.0-only

use slint::ComponentHandle as _;

use crate::actions::ActionId;
use crate::{AppWindow, KeyFeedback};

use super::Adapter;

/// The request-driven switcher flyout's open predicate (UX R-4 D-4,
/// #794): the panel's own facts, not the widget's — the walk's truth
/// is what the window mounted.
pub(in crate::adapter) fn switcher_flyout_open(window: &AppWindow) -> bool {
    let dd = window.global::<crate::DropdownFacts>();
    dd.get_open()
        && dd.get_consumer() == crate::DropdownConsumer::BoardSwitcher
        && window
            .global::<crate::BoardBarFacts>()
            .get_switcher_request()
}

/// The flyout's close (the walk's exits): every fact resets, so the
/// strip returns in strip mode and the ladder declines stop.
pub(in crate::adapter) fn close_switcher_flyout(window: &AppWindow) {
    {
        let facts = window.global::<crate::BoardBarFacts>();
        facts.set_open(false);
        facts.set_switcher_request(false);
    }
    window.global::<crate::DropdownFacts>().set_open(false);
}

impl Adapter {
    /// The flyout walk's key arm (the helper above's engine): the
    /// ladder vocabulary only; anything else is `None` (the resolver
    /// owns it). The clamped highlight moves; Return routes the
    /// highlighted row through the existing segment route.
    pub(crate) fn switcher_flyout_key(
        &mut self,
        window: &AppWindow,
        stroke: crate::keymap::Stroke,
    ) -> Option<bool> {
        // The self-heal: an outside click's underlay wrote the panel's
        // fact directly; the request flags fall in line on the next
        // key (the production loop's changed-delivery syncs them too).
        if window
            .global::<crate::BoardBarFacts>()
            .get_switcher_request()
            && !window.global::<crate::DropdownFacts>().get_open()
        {
            close_switcher_flyout(window);
        }
        if !switcher_flyout_open(window) {
            return None;
        }
        let rows = i32::try_from(slint::Model::row_count(&window.get_board_bar_model().rows))
            .unwrap_or(i32::MAX);
        if rows == 0 {
            return None;
        }
        let named = stroke.named_name();
        let up = named == Some("Up") || (stroke.ctrl && stroke.key == 'p');
        let down = named == Some("Down") || (stroke.ctrl && stroke.key == 'n');
        let ret = named == Some("Return");
        let esc = named == Some("Escape");
        if !(up || down || ret || esc) {
            return None;
        }
        Self::set_feedback(window, KeyFeedback::None);
        let facts = window.global::<crate::BoardBarFacts>();
        if up || down {
            let delta: i32 = if up { -1 } else { 1 };
            let next = (facts.get_highlight() + delta).clamp(0, rows - 1);
            facts.set_highlight(next);
            Some(true)
        } else if esc {
            close_switcher_flyout(window);
            Some(true)
        } else {
            let highlight = facts.get_highlight();
            close_switcher_flyout(window);
            if let Some(index) = usize::try_from(highlight).ok()
                && let Some(board) = self.tables.board_ids.get(index).copied()
            {
                self.board_target = Some(board);
                self.route(window, ActionId::BoardSelect);
            }
            Some(true)
        }
    }
}

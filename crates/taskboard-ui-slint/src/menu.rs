// SPDX-License-Identifier: AGPL-3.0-only

//! The generated action menu (chunk decision D19, design §5.9): a
//! pure projection of the registry over keymap data for a context
//! chain. Menus, hints, and the future keymap visualiser are
//! projections of data, never hand-written (§5.9).

use crate::actions::{Category, ContextId, REGISTRY};
use crate::keymap::Keymap;

/// One generated menu row (D19): the registry entry plus its first
/// chain-visible binding rendered as canonical display strings (the
/// kbd-hint text source). Disabled entries are included — menu
/// generation stays total and documents the reservations (§5.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub action: crate::actions::ActionId,
    pub label: &'static str,
    pub enabled: bool,
    /// The first chain-visible binding, one display string per stroke.
    pub hint: Vec<String>,
    /// The section heading to render above this row (usability D5):
    /// the row category's label at a category boundary (always on the
    /// first row), empty otherwise. Derived from the same stable sort
    /// as the row order — never hand-written (§5.9).
    pub section_title: &'static str,
}

/// Generates the menu rows for `chain` (D19): registry entries whose
/// availability intersects the chain (`Global`-tagged always),
/// ordered by category then registry order.
#[must_use]
pub fn entries(keymap: &Keymap, chain: &[ContextId]) -> Vec<MenuEntry> {
    let mut rows: Vec<(usize, (Category, MenuEntry))> = REGISTRY
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry.contexts.contains(&ContextId::Global)
                || entry.contexts.iter().any(|c| chain.contains(c))
        })
        .map(|(index, entry)| {
            let hint = keymap
                .first_binding_display(entry.id, chain)
                .unwrap_or_default();
            (
                index,
                (
                    entry.category,
                    MenuEntry {
                        action: entry.id,
                        label: entry.label,
                        enabled: entry.enabled,
                        hint,
                        section_title: "",
                    },
                ),
            )
        })
        .collect();
    // Stable sort by (category, registry order): `Category` orders by
    // its declaration (the documented menu order).
    rows.sort_by_key(|(index, (category, _))| (*category, *index));
    // The section boundaries are derived data (usability D5): a row
    // heading is its category's label exactly where the category
    // changes (the first row always opens its section).
    let mut previous = Option::<Category>::None;
    rows.into_iter()
        .map(|(_, (category, mut row))| {
            row.section_title = if previous == Some(category) {
                ""
            } else {
                category.section_label()
            };
            previous = Some(category);
            row
        })
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

    /// T24 — filtering: availability intersects the chain
    /// (`Global`-tagged always); a view chain offers the view's verbs
    /// but no `menu.*` rows, and nothing appears twice.
    #[test]
    fn entries_filter_to_the_chain_and_global() {
        let keymap = standard_keymap();
        let board = entries(&keymap, &[ContextId::Board, ContextId::Global]);
        let ids: Vec<_> = board.iter().map(|e| e.action).collect();
        assert!(ids.contains(&ActionId::TaskComplete), "board verb offered");
        assert!(
            !ids.contains(&ActionId::MenuNext),
            "menu cursor not in a view menu"
        );
        assert_eq!(
            ids.len(),
            std::collections::BTreeSet::<_>::from_iter(ids.iter()).len()
        );

        // A Global-only chain (chunk-2 sessions) still gets the
        // global rows — the menu generation stays total (§5.8).
        let global = entries(&keymap, &[ContextId::Global]);
        assert!(global.iter().any(|e| e.action == ActionId::AppExit));
        assert!(!global.iter().any(|e| e.action == ActionId::TaskComplete));
    }

    /// T24 — ordering: by category (declaration order) then registry
    /// order; rows carry the first chain-visible binding as display
    /// strings; disabled entries are included (greyed, §5.8).
    #[test]
    fn entries_are_ordered_carry_hints_and_disabled_rows() {
        let keymap = standard_keymap();
        let board = entries(&keymap, &[ContextId::Board, ContextId::Global]);

        let categories: Vec<Category> = board
            .iter()
            .map(|e| {
                REGISTRY
                    .iter()
                    .find(|r| r.id == e.action)
                    .expect("row")
                    .category
            })
            .collect();
        let mut sorted = categories.clone();
        sorted.sort();
        assert_eq!(categories, sorted, "category-ordered");

        let exit = board
            .iter()
            .find(|e| e.action == ActionId::AppExit)
            .expect("global");
        assert!(exit.enabled);
        assert_eq!(exit.hint, vec!["Ctrl+Q".to_owned()]);

        let undo = board
            .iter()
            .find(|e| e.action == ActionId::AppUndo)
            .expect("reserved");
        assert!(!undo.enabled, "reserved rows read disabled");
        assert_eq!(
            undo.hint,
            vec!["Ctrl+Z".to_owned()],
            "bound-but-disabled still hinted"
        );

        let emacs = build(&RawKeymap {
            preset: Preset::Emacs,
            overrides: BTreeMap::new(),
        })
        .keymap;
        let board = entries(&emacs, &[ContextId::Board, ContextId::Global]);
        let exit = board
            .iter()
            .find(|e| e.action == ActionId::AppExit)
            .expect("global");
        assert_eq!(
            exit.hint,
            vec!["Ctrl+x".to_owned(), "Ctrl+c".to_owned()],
            "chord hinted stroke-by-stroke"
        );
    }

    /// The projection over the real chunk-2 session chain: the mode
    /// context is always a chain member (D10/D11) and standard/emacs
    /// leave it unbound — hints must still resolve from Global.
    #[test]
    fn entries_carry_hints_over_the_mode_context_session_chain() {
        for preset in [Preset::Standard, Preset::Emacs] {
            let keymap = build(&RawKeymap {
                preset,
                overrides: BTreeMap::new(),
            })
            .keymap;
            let rows = entries(&keymap, &[ContextId::ViNormal, ContextId::Global]);
            let exit = rows
                .iter()
                .find(|e| e.action == ActionId::AppExit)
                .expect("app.exit is global-tagged");
            assert!(
                !exit.hint.is_empty(),
                "{preset:?}: every resolvable global binding is hinted"
            );
        }
    }
}

#[cfg(test)]
mod section_tests {
    use super::*;
    use crate::actions::ActionId;
    use crate::keymap::{Preset, RawKeymap, build};
    use std::collections::BTreeMap;

    /// G5 — sections are derived data (usability D5): a row carries a
    /// non-empty section title exactly at a category boundary (and on
    /// the first row), and the section count equals the count of
    /// distinct categories present. The menu stays a projection —
    /// nothing hand-written.
    #[test]
    fn entries_carry_section_titles_at_category_boundaries() {
        let keymap = build(&RawKeymap {
            preset: Preset::Standard,
            overrides: BTreeMap::new(),
        })
        .keymap;
        let rows = entries(&keymap, &[ContextId::Board, ContextId::Global]);
        assert_ne!(rows, [] as [MenuEntry; 0]);
        let mut previous = Option::<Category>::None;
        let mut titled = 0;
        for row in &rows {
            let category = REGISTRY
                .iter()
                .find(|r| r.id == row.action)
                .expect("row")
                .category;
            let boundary = previous != Some(category);
            assert_eq!(
                row.section_title.is_empty(),
                !boundary,
                "section title exactly at the {category:?} boundary"
            );
            assert_eq!(
                row.section_title,
                if boundary {
                    category.section_label()
                } else {
                    ""
                }
            );
            previous = Some(category);
            titled += usize::from(!row.section_title.is_empty());
        }
        let categories: std::collections::BTreeSet<Category> = rows
            .iter()
            .map(|row| {
                REGISTRY
                    .iter()
                    .find(|r| r.id == row.action)
                    .expect("row")
                    .category
            })
            .collect();
        assert_eq!(titled, categories.len(), "one section per present category");
        assert!(
            rows.iter().any(|row| row.action == ActionId::AppExit
                && row.section_title == Category::Application.section_label()),
            "the first row always opens its section"
        );
    }
}

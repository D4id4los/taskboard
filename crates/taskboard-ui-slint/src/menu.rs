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
                    },
                ),
            )
        })
        .collect();
    // Stable sort by (category, registry order): `Category` orders by
    // its declaration (the documented menu order).
    rows.sort_by_key(|(index, (category, _))| (*category, *index));
    rows.into_iter().map(|(_, (_, row))| row).collect()
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

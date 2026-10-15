// SPDX-License-Identifier: AGPL-3.0-only

//! The keymap-as-data model (design §5.3): a preset table ⊕ user
//! overrides, validated into an effective [`Keymap`]. Everything here
//! is pure — parsing, merging, and validation never touch slint types
//! or IO (the §5.5 extraction seam); config is read app-side and
//! handed over as typed values.

pub mod presets;
pub mod stroke;

pub use presets::Preset;
pub use stroke::{Stroke, StrokeParseError};

use std::collections::{BTreeMap, BTreeSet};

use crate::actions::{ActionId, ContextId};
/// The effective keymap: per binding context, stroke sequences →
/// actions. `BTreeMap` ordering makes snapshots and the menu's
/// first-binding display deterministic (D15).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keymap {
    bindings: BTreeMap<ContextId, BTreeMap<Vec<Stroke>, ActionId>>,
    /// Contexts declared as masking (D11): a chain walk that reaches
    /// one stops there.
    masks: BTreeSet<ContextId>,
}

impl Keymap {
    /// The keymap of a shipped preset with no overrides (the resolved
    /// default for that preset).
    ///
    /// # Panics
    ///
    /// Never in a reviewed tree: preset spellings are const data
    /// parsed here; a malformed spelling in a preset table is a
    /// broken-table bug the contract tests catch (D21).
    #[must_use]
    pub fn from_preset(preset: Preset) -> Self {
        let mut keymap = Self::default();
        keymap.masks.extend(preset.masks().iter().copied());
        for (action, context, spellings) in preset.rows() {
            let seq = stroke::parse_sequence(spellings.iter().copied())
                .expect("preset spellings parse (contract-tested)");
            keymap.insert(*action, *context, seq);
        }
        keymap
    }

    fn insert(&mut self, action: ActionId, context: ContextId, seq: Vec<Stroke>) {
        self.bindings
            .entry(context)
            .or_default()
            .insert(seq, action);
    }

    /// Every binding context that carries at least one binding
    /// (snapshot/menu ordering input).
    pub fn contexts(&self) -> impl Iterator<Item = ContextId> + '_ {
        self.bindings.keys().copied()
    }

    /// The declared masking contexts.
    pub fn masks(&self) -> impl Iterator<Item = ContextId> + '_ {
        self.masks.iter().copied()
    }

    /// The bindings of one context (raw access for snapshots).
    #[must_use]
    pub fn bindings_in(&self, context: ContextId) -> Option<&BTreeMap<Vec<Stroke>, ActionId>> {
        self.bindings.get(&context)
    }

    /// The chain with masking applied (D11/D12): contexts up to and
    /// including the first masking context.
    #[must_use]
    pub fn visible_chain(&self, chain: &[ContextId]) -> Vec<ContextId> {
        let mut visible = Vec::with_capacity(chain.len());
        for context in chain {
            visible.push(*context);
            if self.masks.contains(context) {
                break;
            }
        }
        visible
    }

    /// First-match resolution over a chain (D11): the highest visible
    /// context carrying exactly `seq` wins.
    #[must_use]
    pub fn resolve(&self, chain: &[ContextId], seq: &[Stroke]) -> Option<ActionId> {
        for context in self.visible_chain(chain) {
            if let Some(action) = self.bindings.get(&context).and_then(|b| b.get(seq)) {
                return Some(*action);
            }
        }
        None
    }

    /// Whether any visible binding's sequence *starts with* `seq`
    /// (the resolver's pending test, D12 rule 3).
    #[must_use]
    pub fn has_prefix(&self, chain: &[ContextId], seq: &[Stroke]) -> bool {
        for context in self.visible_chain(chain) {
            if self
                .bindings
                .get(&context)
                .is_some_and(|bindings| bindings.keys().any(|c| c.starts_with(seq)))
            {
                return true;
            }
        }
        false
    }

    /// The first chain-visible binding of `action`, as canonical
    /// display spellings — the kbd-hint text source (D19; menus and
    /// hints are projections of keymap data, never hand-written).
    #[must_use]
    pub fn first_binding_display(
        &self,
        action: ActionId,
        chain: &[ContextId],
    ) -> Option<Vec<String>> {
        for context in self.visible_chain(chain) {
            let bindings = self.bindings.get(&context)?;
            if let Some(seq) = bindings
                .iter()
                .filter(|(_, bound)| **bound == action)
                .map(|(seq, _)| seq)
                .min()
            {
                return Some(seq.iter().map(ToString::to_string).collect());
            }
        }
        None
    }

    /// The contexts where the preset bound `action` (the override
    /// placement rule, D9): empty means the preset left it unbound.
    #[must_use]
    pub fn bound_contexts(&self, action: ActionId) -> Vec<ContextId> {
        self.bindings
            .iter()
            .filter(|(_, bindings)| bindings.values().any(|bound| *bound == action))
            .map(|(context, _)| *context)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionId;
    use std::collections::BTreeMap;

    /// Every preset spelling parses (the `from_preset` expects stay
    /// unreachable in a reviewed tree).
    #[test]
    fn every_preset_row_parses() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            for (action, context, spellings) in preset.rows() {
                let parsed = stroke::parse_sequence(spellings.iter().copied());
                assert!(
                    parsed.is_ok(),
                    "{preset:?} {action} in {context}: unparsable spellings"
                );
            }
        }
    }

    /// T6 — core-verb coverage (D8/D9/D21): every core verb has ≥ 1
    /// binding in every preset, so no preset is a second-class
    /// citizen (Q14). The vi preset additionally carries the mode
    /// flips (§5.4).
    #[test]
    fn every_preset_covers_the_core_verbs() {
        let core = [
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::AppMenu,
            ActionId::AppExit,
            ActionId::CaptureQuick,
            ActionId::NavNextItem,
            ActionId::NavPrevItem,
            ActionId::NavNextView,
            ActionId::NavPrevView,
            ActionId::TaskCreate,
            ActionId::TaskEdit,
            ActionId::TaskComplete,
            ActionId::TaskMove,
            ActionId::TaskDelete,
            ActionId::MenuNext,
            ActionId::MenuPrevious,
            ActionId::MenuInvoke,
            ActionId::MenuDismiss,
        ];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let bound: std::collections::BTreeSet<ActionId> =
                preset.rows().iter().map(|(a, _, _)| *a).collect();
            for verb in core {
                assert!(bound.contains(&verb), "{preset:?} misses core verb {verb}");
            }
        }
        let vi_bound: std::collections::BTreeSet<ActionId> =
            Preset::Vi.rows().iter().map(|(a, _, _)| *a).collect();
        for verb in [ActionId::ModeInsert, ActionId::ModeNormal] {
            assert!(vi_bound.contains(&verb), "vi misses {verb}");
        }
    }

    /// Grouped rows per context for the conflict/shadow contracts.
    fn rows_by_context(preset: Preset) -> BTreeMap<ContextId, Vec<(ActionId, Vec<Stroke>)>> {
        let mut grouped: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (action, context, spellings) in preset.rows() {
            let seq = stroke::parse_sequence(spellings.iter().copied())
                .expect("rows parse (tested above)");
            grouped.entry(*context).or_default().push((*action, seq));
        }
        grouped
    }

    /// T7 — intra-context conflict-freeness (D21): no two actions
    /// share a sequence in one context. Replaces the compiler-side
    /// duplicate detection `KeyBinding` would have given (§5.7a,
    /// Q9).
    #[test]
    fn presets_are_intra_context_conflict_free() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            for (context, rows) in rows_by_context(preset) {
                for (i, (a, seq_a)) in rows.iter().enumerate() {
                    for (b, seq_b) in rows.iter().skip(i + 1) {
                        assert_ne!(seq_a, seq_b, "{preset:?} conflict in {context}: {a} vs {b}");
                    }
                }
            }
        }
    }

    /// T8 — prefix-shadow-freeness (D13/D21): no same-context
    /// binding is a strict prefix of another (usable, just lossy —
    /// and shipped presets are not).
    #[test]
    fn presets_are_prefix_shadow_free() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            for (context, rows) in rows_by_context(preset) {
                for (i, (a, seq_a)) in rows.iter().enumerate() {
                    for (b, seq_b) in rows.iter().skip(i + 1) {
                        let (prefix_action, prefix_seq, other_action, other_seq) =
                            if seq_a.len() <= seq_b.len() {
                                (*a, seq_a, *b, seq_b)
                            } else {
                                (*b, seq_b, *a, seq_a)
                            };
                        assert!(
                            !other_seq.starts_with(prefix_seq),
                            "{preset:?} shadow in {context}: {prefix_action:?} is a prefix of {other_action:?}"
                        );
                    }
                }
            }
        }
    }

    /// The vi model (D8/D10): `ViNormal` carries everything, Global
    /// is empty, `ViInsert` binds only the return-to-normal flip.
    #[test]
    fn vi_preset_shape_matches_the_mode_model() {
        let by_context = rows_by_context(Preset::Vi);
        assert!(!by_context.contains_key(&ContextId::Global));
        assert_eq!(
            by_context
                .get(&ContextId::ViInsert)
                .expect("vi binds insert")
                .len(),
            1
        );
        assert!(
            by_context
                .get(&ContextId::ViNormal)
                .expect("vi binds normal")
                .len()
                > 1
        );
    }

    /// Masking declarations (D8/D11): every preset masks below the
    /// menu only; `ViInsert` shadows without masking.
    #[test]
    fn presets_mask_only_the_menu() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let masks: Vec<_> = preset.masks().to_vec();
            assert_eq!(masks, vec![ContextId::Menu]);
        }
    }

    /// `from_preset` yields the map; empty overrides are tested at
    /// `build` (T16) — here just the round shape.
    #[test]
    fn from_preset_builds_a_total_map() {
        let keymap = Keymap::from_preset(Preset::Standard);
        assert_eq!(
            keymap.resolve(
                &[ContextId::Board],
                &[stroke::parse("Space").expect("space")]
            ),
            Some(ActionId::TaskComplete)
        );
        // Global's Ctrl+Z resolves under a view chain that carries Global.
        assert_eq!(
            keymap.resolve(
                &[ContextId::Board, ContextId::Global],
                &[stroke::parse("Ctrl+Z").expect("ctrl z")]
            ),
            Some(ActionId::AppUndo)
        );
    }
}

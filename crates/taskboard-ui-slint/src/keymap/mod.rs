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
    ///
    /// An unbound context is skipped, not terminal: the shell's chain
    /// always carries the current mode context, which the standard and
    /// emacs presets leave empty (D10/D11).
    #[must_use]
    pub fn first_binding_display(
        &self,
        action: ActionId,
        chain: &[ContextId],
    ) -> Option<Vec<String>> {
        for context in self.visible_chain(chain) {
            let Some(bindings) = self.bindings.get(&context) else {
                continue;
            };
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

/// The raw, pre-validation keymap input (the config layer stores
/// strings only — structure there, semantics here): a preset plus the
/// user's flat per-action override deltas (D9).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawKeymap {
    pub preset: Preset,
    /// Action-id string → override sequences (each a list of stroke
    /// spellings; the bare-string sugar is normalized to one
    /// single-stroke sequence app-side, D7).
    pub overrides: BTreeMap<String, Vec<Vec<String>>>,
}

/// Why a validation finding was raised. Typed enum values only —
/// nothing downstream may match on rendered text (AGENTS §5, Q15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// The override's action id is not in the registry.
    UnknownAction,
    /// A stroke spelling does not parse (D6).
    MalformedStroke,
    /// An add-new override would land where the action can never fire
    /// (D9: preset-unbound, registry availability lacks `Global`).
    MaskedContext,
    /// The override's sequence collides intra-context with another
    /// effective binding; the override drops (both sides when two
    /// overrides collide — no last-wins).
    Conflict,
    /// A same-context binding is a strict prefix of another (§5.7):
    /// warning only, nothing drops.
    PrefixShadow,
}

/// One validation finding, fully typed (D13): what, where, and the
/// offending stroke in canonical display form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    pub action: Option<ActionId>,
    pub context: ContextId,
    pub stroke: String,
}

/// The validated result (Q15): an always-usable keymap — offending
/// overrides dropped, preset bindings surviving verbatim — plus the
/// exhaustive findings for `tracing` surfacing and the startup-
/// warning state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltKeymap {
    pub keymap: Keymap,
    pub findings: Vec<Finding>,
}

fn seq_display(seq: &[Stroke]) -> String {
    seq.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds the effective keymap: preset ⊕ overrides with drop-and-warn
/// validation (D9, D13). Pure and total — a hostile keymap can never
/// block UI start (Q15); findings are the only output channel for
/// what was dropped or is merely lossy.
#[must_use]
pub fn build(raw: &RawKeymap) -> BuiltKeymap {
    let mut keymap = Keymap::from_preset(raw.preset);
    let findings = apply_overrides(&mut keymap, &raw.overrides);
    BuiltKeymap { keymap, findings }
}

/// Applies the user override deltas onto `keymap` in place, returning
/// the exhaustive findings. The separation keeps `build` a
/// composition of named operations (preset base, validate-and-stage,
/// apply, shadow pass).
fn apply_overrides(
    keymap: &mut Keymap,
    overrides: &BTreeMap<String, Vec<Vec<String>>>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Cleanly-validated overrides, in config order; a conflict evicts
    // an earlier one (both-drop, D13). Nothing is applied until the
    // whole pass is staged, so an eviction restores the preset
    // verbatim.
    let mut staged: Vec<(ActionId, ContextId, Vec<Stroke>)> = Vec::new();

    for (id, sequences) in overrides {
        // One call: reads the keymap and the pass-so-far (`staged` is
        // the applied-prefix), appends findings and sequences.
        place_override(keymap, &mut staged, id, sequences, &mut findings);
    }

    apply_staged(keymap, &staged);
    collect_prefix_shadows(keymap, &mut findings);
    findings
}

/// Lands the staged overrides (D9's replace-and-insert): every staged
/// sequence is inserted, then in each touched (action, context) pair
/// the action's other bindings are removed — an override *replaces*
/// the preset's bindings of its action. Staging the whole pass first
/// is what makes the replacement total and deterministic.
fn apply_staged(keymap: &mut Keymap, staged: &[(ActionId, ContextId, Vec<Stroke>)]) {
    let touched: BTreeSet<(ActionId, ContextId)> = staged
        .iter()
        .map(|(action, context, _)| (*action, *context))
        .collect();
    for (action, context, seq) in staged {
        keymap.insert(*action, *context, seq.clone());
    }
    for (action, context) in &touched {
        let keep: Vec<Vec<Stroke>> = staged
            .iter()
            .filter(|(a, c, _)| a == action && c == context)
            .map(|(_, _, s)| s.clone())
            .collect();
        if let Some(bindings) = keymap.bindings.get_mut(context) {
            let stale: Vec<Vec<Stroke>> = bindings
                .iter()
                .filter(|(seq, bound)| **bound == *action && !keep.contains(seq))
                .map(|(seq, _)| seq.clone())
                .collect();
            for seq in stale {
                bindings.remove(&seq);
            }
        }
    }
}

/// Validates and stages one override entry (D9 placement, D13
/// findings); staged sequences land in `applied`, which the caller
/// applies after the whole pass.
fn place_override(
    keymap: &Keymap,
    staged: &mut Vec<(ActionId, ContextId, Vec<Stroke>)>,
    id: &str,
    sequences: &[Vec<String>],
    findings: &mut Vec<Finding>,
) {
    let Some(action) = ActionId::from_id(id) else {
        findings.push(Finding {
            kind: FindingKind::UnknownAction,
            action: None,
            context: ContextId::Global,
            stroke: id.to_owned(),
        });
        return;
    };
    // Placement (D9): an override replaces the preset's bindings of
    // the action, in the preset's own binding contexts; an action
    // the map leaves unbound is *added* in `Global` — where
    // `MaskedContext` drops it if it could never fire there.
    let bound_contexts = keymap.bound_contexts(action);
    let add_new = bound_contexts.is_empty();
    let targets: Vec<ContextId> = if add_new {
        vec![ContextId::Global]
    } else {
        bound_contexts.clone()
    };
    let mut parsed: Vec<Vec<Stroke>> = Vec::new();
    for spellings in sequences {
        match stroke::parse_sequence(spellings.iter().map(String::as_str)) {
            Ok(seq) if !seq.is_empty() => parsed.push(seq),
            _ => findings.push(Finding {
                kind: FindingKind::MalformedStroke,
                action: Some(action),
                context: targets[0],
                stroke: spellings.join(" "),
            }),
        }
    }
    for context in targets {
        if add_new && !crate::actions::entry(action).contexts.contains(&context) {
            findings.push(Finding {
                kind: FindingKind::MaskedContext,
                action: Some(action),
                context,
                stroke: parsed
                    .first()
                    .map(|seq| seq_display(seq))
                    .unwrap_or_default(),
            });
            continue;
        }
        for seq in &parsed {
            // The action's own current binding in this context is
            // being replaced, never a conflict; anything else holding
            // the sequence is.
            let holder = keymap
                .bindings_in(context)
                .and_then(|bindings| bindings.get(seq))
                .copied();
            let staged_conflict = staged
                .iter()
                .position(|(_, c, s)| *c == context && s == seq);
            match (holder, staged_conflict) {
                // User-vs-preset (a *different* action holds the
                // sequence): the override drops, the preset binding
                // survives verbatim (D13).
                (Some(holder), _) if holder != action => {
                    findings.push(Finding {
                        kind: FindingKind::Conflict,
                        action: Some(action),
                        context,
                        stroke: seq_display(seq),
                    });
                }
                // User-vs-user: both overrides drop, the preset wins
                // for both — deterministic, no last-wins.
                (_, Some(pos)) => {
                    let (earlier, earlier_context, earlier_seq) = staged.remove(pos);
                    findings.push(Finding {
                        kind: FindingKind::Conflict,
                        action: Some(earlier),
                        context: earlier_context,
                        stroke: seq_display(&earlier_seq),
                    });
                    findings.push(Finding {
                        kind: FindingKind::Conflict,
                        action: Some(action),
                        context,
                        stroke: seq_display(seq),
                    });
                }
                _ => staged.push((action, context, seq.clone())),
            }
        }
    }
}

/// The prefix-shadow pass (D13): computed on the *effective* map,
/// warning only — a prefix binding is usable, just lossy (§5.7).
fn collect_prefix_shadows(keymap: &Keymap, findings: &mut Vec<Finding>) {
    for (context, bindings) in &keymap.bindings {
        for (short, short_action) in bindings {
            for long in bindings.keys() {
                if long.len() > short.len() && long.starts_with(short) {
                    findings.push(Finding {
                        kind: FindingKind::PrefixShadow,
                        action: Some(*short_action),
                        context: *context,
                        stroke: seq_display(short),
                    });
                }
            }
        }
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
            ActionId::BoardNext,
            ActionId::BoardPrevious,
            ActionId::StackWider,
            ActionId::StackNarrower,
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

    /// Design §3.8 (R4: "navigate boards" is a core verb): every
    /// preset resolves both board verbs in a `[Board, ViNormal,
    /// Global]` chain — standard/emacs bind them in `Board`, vi in
    /// `ViNormal`. The exact strokes are the preset draft's business;
    /// the contract is existence + resolution.
    #[test]
    fn every_preset_resolves_the_board_verbs_in_the_view_chain() {
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for verb in [ActionId::BoardNext, ActionId::BoardPrevious] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the board chain"
                );
            }
        }
    }

    /// Round-1 chunk 2 (D18): every preset resolves both stack-width
    /// verbs in the `[Board, ViNormal, Global]` chain — standard/emacs
    /// bind them in `Board`, vi in `ViNormal`.
    #[test]
    fn every_preset_resolves_the_stack_width_verbs_in_the_board_chain() {
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for verb in [ActionId::StackWider, ActionId::StackNarrower] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the board chain"
                );
            }
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

    /// The kbd-hint lookup walks the whole chain: a context the preset
    /// leaves unbound (the always-present mode context, D10/D11) is
    /// skipped, not terminal — the real chunk-2 session chain is
    /// `[ViNormal, Global]` under the standard preset.
    #[test]
    fn first_binding_display_skips_unbound_chain_contexts() {
        let keymap = Keymap::from_preset(Preset::Standard);
        let session_chain = [ContextId::ViNormal, ContextId::Global];
        let seq = keymap
            .first_binding_display(ActionId::AppExit, &session_chain)
            .expect("the Global binding is found past the empty mode context");
        assert_eq!(seq, vec!["Ctrl+Q".to_owned()]);

        // An action bound nowhere in the chain is still `None`.
        assert_eq!(
            keymap.first_binding_display(ActionId::MenuNext, &session_chain),
            None
        );
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::actions::ContextId;
    use FindingKind::*;

    fn built(preset: Preset, overrides: &[(&str, &[&[&str]])]) -> BuiltKeymap {
        build(&RawKeymap {
            preset,
            overrides: overrides
                .iter()
                .map(|(id, seqs)| {
                    (
                        (*id).to_owned(),
                        seqs.iter()
                            .map(|s| s.iter().map(|t| (*t).to_owned()).collect())
                            .collect(),
                    )
                })
                .collect(),
        })
    }

    fn one(result: &BuiltKeymap, kind: FindingKind) -> &Finding {
        result
            .findings
            .iter()
            .find(|f| f.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} finding in {:?}", result.findings))
    }

    /// T9 — an override replaces *every* binding of the action (D9):
    /// the old sequences stop resolving in all of the preset's
    /// binding contexts, the new one resolves in each.
    #[test]
    fn override_replaces_all_bindings_of_the_action() {
        let result = built(Preset::Standard, &[("task.complete", &[&["Ctrl+Space"]])]);
        assert_eq!(result.findings, Vec::new());
        let space = stroke::parse("Space").expect("space");
        let ctrl_space = stroke::parse("Ctrl+Space").expect("ctrl space");
        // The standard preset binds `task.complete` in the board
        // context (D8 draft); that is the context the override
        // replaces.
        let chain = [ContextId::Board, ContextId::Global];
        assert_eq!(
            result.keymap.resolve(&chain, &[ctrl_space]),
            Some(ActionId::TaskComplete)
        );
        assert_eq!(
            result.keymap.resolve(&chain, &[space]),
            None,
            "old binding gone"
        );
    }

    /// T10 — the add-new rule (D9): an action the current map leaves
    /// unbound gains its binding in `Global` — unless its registry
    /// availability excludes `Global`, in which case the entry can
    /// never fire and drops as `MaskedContext`. Exercised on a
    /// synthetic base (an empty map is the upgrade-model case: a
    /// preset that binds nothing yet), since every *shipped* preset
    /// covers the non-global actions by the coverage contract.
    #[test]
    fn add_new_binds_global_but_drops_masked_context() {
        let override_entry =
            |id: &str| BTreeMap::from([(id.to_owned(), vec![vec!["Ctrl+0".to_owned()]])]);
        let seq = stroke::parse_sequence(["Ctrl+0"]).expect("parses");

        // `mode.normal` is globally available: the add-new lands.
        let mut keymap = Keymap::default();
        let findings = apply_overrides(&mut keymap, &override_entry("mode.normal"));
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(
            keymap.resolve(&[ContextId::Global], &seq),
            Some(ActionId::ModeNormal)
        );

        // `menu.next` is only available in the menu context — a
        // Global-placed binding would be inert, so it drops typed.
        let mut keymap = Keymap::default();
        let findings = apply_overrides(&mut keymap, &override_entry("menu.next"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, MaskedContext);
        assert_eq!(findings[0].action, Some(ActionId::MenuNext));
        assert_eq!(keymap.resolve(&[ContextId::Global], &seq), None);
    }

    /// T11 — `UnknownAction` and `MalformedStroke` drop only the
    /// offender; other overrides in the same config survive (T15's
    /// exhaustiveness separately).
    #[test]
    fn unknown_and_malformed_drop_only_the_offender() {
        let result = built(
            Preset::Standard,
            &[
                ("no.such.action", &[&["X"]]),
                ("task.complete", &[&["Ctrl+!!"]]),
                ("app.exit", &[&["Ctrl+\\"]]),
            ],
        );
        assert_eq!(one(&result, UnknownAction).stroke, "no.such.action");
        assert_eq!(
            one(&result, MalformedStroke).action,
            Some(ActionId::TaskComplete)
        );
        let seq = stroke::parse_sequence(["Ctrl+\\"]).expect("parses");
        assert_eq!(
            result.keymap.resolve(&[ContextId::Global], &seq),
            Some(ActionId::AppExit),
            "the valid override survived"
        );
    }

    /// T12 — the Q15 redirect, fallback asserted (not just the drop):
    /// an override colliding with a preset binding drops and the
    /// preset binding resolves verbatim.
    #[test]
    fn conflict_with_preset_drops_override_preset_survives() {
        let result = built(Preset::Standard, &[("app.undo", &[&["Ctrl+Q"]])]);
        let finding = one(&result, Conflict);
        assert_eq!(finding.action, Some(ActionId::AppUndo));
        assert_eq!(finding.context, ContextId::Global);
        let undo = stroke::parse("Ctrl+Z").expect("undo");
        let quit = stroke::parse("Ctrl+Q").expect("quit");
        assert_eq!(
            result.keymap.resolve(&[ContextId::Global], &[undo]),
            Some(ActionId::AppUndo),
            "preset fallback intact"
        );
        assert_eq!(
            result.keymap.resolve(&[ContextId::Global], &[quit]),
            Some(ActionId::AppExit),
            "the colliding preset binding untouched"
        );
    }

    /// T13 — user-vs-user conflict: both overrides drop, preset wins
    /// for both (deterministic, no last-wins).
    #[test]
    fn conflict_between_overrides_drops_both() {
        let result = built(
            Preset::Standard,
            &[("app.undo", &[&["Ctrl+\\"]]), ("app.redo", &[&["Ctrl+\\"]])],
        );
        let conflict_actions: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.kind == Conflict)
            .map(|f| f.action)
            .collect();
        assert!(conflict_actions.contains(&Some(ActionId::AppUndo)));
        assert!(conflict_actions.contains(&Some(ActionId::AppRedo)));
        let undo = stroke::parse("Ctrl+Z").expect("undo");
        let redo = stroke::parse_sequence(["Ctrl+Shift+Z"]).expect("redo");
        let seq = stroke::parse("Ctrl+\\").expect("x");
        assert_eq!(
            result.keymap.resolve(&[ContextId::Global], &[undo]),
            Some(ActionId::AppUndo)
        );
        assert_eq!(
            result.keymap.resolve(&[ContextId::Global], &redo),
            Some(ActionId::AppRedo)
        );
        assert_eq!(result.keymap.resolve(&[ContextId::Global], &[seq]), None);
    }

    /// T14 — `PrefixShadow` is computed on the effective map and is a
    /// warning only: nothing drops, the binding still resolves.
    #[test]
    fn prefix_shadow_warns_without_dropping() {
        // `Ctrl+x` alone is not bound in emacs; binding it shadows
        // the whole C-x family.
        let result = built(Preset::Emacs, &[("app.help", &[&["Ctrl+x"]])]);
        let shadow = one(&result, PrefixShadow);
        assert_eq!(shadow.context, ContextId::Global);
        let seq = stroke::parse("Ctrl+x").expect("x");
        assert_eq!(
            result.keymap.resolve(&[ContextId::Global], &[seq]),
            Some(ActionId::AppHelp),
            "shadowing binding still resolves"
        );
    }

    /// T15 — findings are exhaustive: multiple offenders in one
    /// config are all listed.
    #[test]
    fn findings_are_exhaustive() {
        let result = built(
            Preset::Standard,
            &[
                ("ghost.action", &[&["X"]]),
                ("task.complete", &[&["Ctrl+!!"]]),
                ("app.undo", &[&["Ctrl+Q"]]),
            ],
        );
        // `MaskedContext` is covered by T10's synthetic base (the
        // shipped presets cover the non-global actions by contract).
        let kinds: Vec<_> = result.findings.iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&UnknownAction));
        assert!(kinds.contains(&MalformedStroke));
        assert!(kinds.contains(&Conflict));
    }

    /// T16 — empty overrides ≡ the preset map.
    #[test]
    fn empty_overrides_equal_the_preset_map() {
        let result = built(Preset::Vi, &[]);
        assert_eq!(result.keymap, Keymap::from_preset(Preset::Vi));
        assert_eq!(result.findings, Vec::new());
    }
}

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
            keymap.insert(action, context, seq);
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
    ///
    /// The chord-shift normalization (keymap-parity plan D-3, N-17;
    /// made directional by the operator's Dvorak-International report
    /// — issue #855) rides additive behind the exact pass: when no
    /// context carries `seq` exactly and the sequence's last stroke
    /// yields [`Stroke::chord_shift_partners`] candidates, the lookup
    /// retries once per candidate with it substituted for the last
    /// stroke (the Dvorak-International shape: the token spells
    /// `Alt+<`, the layout delivers the base glyph under Alt+Shift).
    /// An unshifted chorded event (bare Alt+,) yields no candidates —
    /// the pre-fix symmetric superset let it steal the begin/end
    /// tokens. `has_prefix` stays exact — the chord grammar (pending
    /// prefixes) cannot be altered by the map.
    #[must_use]
    pub fn resolve(&self, chain: &[ContextId], seq: &[Stroke]) -> Option<ActionId> {
        for context in self.visible_chain(chain) {
            if let Some(action) = self.bindings.get(&context).and_then(|b| b.get(seq)) {
                return Some(*action);
            }
        }
        let last = seq.len().checked_sub(1)?;
        let mut resolved = None;
        for partner in seq[last].chord_shift_partners() {
            let mut normalized = seq.to_vec();
            normalized[last] = partner;
            for context in self.visible_chain(chain) {
                if let Some(action) = self.bindings.get(&context).and_then(|b| b.get(&normalized)) {
                    resolved = Some(*action);
                    break;
                }
            }
            if resolved.is_some() {
                break;
            }
        }
        resolved
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

    /// Chunk 8 (D4/D7, #934): the habit keyboard rows resolve their
    /// strokes through the full `[Today, Global]` chain in every
    /// preset — the shifted arrows reach the keymap intact (the
    /// arrows' shift-fold leniency covers printable rows only), and
    /// the chorded letters carry the chunk-1 case-law spelling.
    #[test]
    fn every_preset_resolves_the_habit_keyboard_rows_in_the_today_chain() {
        let chain = [ContextId::Today, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for verb in [
                ActionId::HabitMoveUp,
                ActionId::HabitMoveDown,
                ActionId::HabitConvertArm,
                ActionId::HabitUndoLog,
                // US-8 (W-9, #1057): the repair chord rides the same
                // Today-context parity law.
                ActionId::HabitRepair,
                // D3a (W-5/W-16, #1202): the log chord and the Return
                // activation row — the keyboard-first law's explicit
                // rows, in every preset (the no-second-class gate).
                ActionId::HabitLog,
                ActionId::HabitEdit,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the today chain"
                );
            }
        }
    }

    /// Chunk 8 (D7): the four new Today rows introduce no stroke
    /// collision inside the Today context — one stroke, one binding
    /// per context is the grammar the translation (D3) composes
    /// against.
    #[test]
    fn the_today_context_holds_no_duplicate_strokes_after_the_habit_rows() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let mut seen = std::collections::BTreeMap::new();
            for (action, context, spellings) in preset.rows() {
                if context != ContextId::Today {
                    continue;
                }
                // The sequence's full spelling list is the identity —
                // a shared first key with a longer sequence (the
                // emacs `Ctrl+c t` family's exact-match-first
                // prefixing) is the documented leniency, not a
                // collision.
                let sequence = spellings.join("\u{1f}");
                if let Some(first) = seen.insert(sequence.clone(), action) {
                    assert_eq!(
                        first, action,
                        "{preset:?}: {sequence:?} binds both {first} and {action} in Today"
                    );
                }
            }
        }
    }

    /// US-11/US-12 (W-12/W-13, #1067/#1068): the series chords and the
    /// tracker-kind toggle resolve in each preset's detail chain — and
    /// the kind toggle's binding is distinct from `TaskTrackToggle`'s
    /// (the R-8 smart verb) in every preset.
    #[test]
    fn the_series_and_kind_toggle_rows_resolve_in_the_detail_chains() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            let chain: Vec<ContextId> = match preset {
                Preset::Vi => vec![
                    ContextId::TaskDetail,
                    ContextId::ViNormal,
                    ContextId::Global,
                ],
                _ => vec![ContextId::TaskDetail, ContextId::Global],
            };
            for verb in [
                ActionId::SeriesPause,
                ActionId::SeriesResume,
                ActionId::TaskToggleTracker,
                // US-10 (W-11, #1091): the disarm chord rides the same
                // detail-chain parity law.
                ActionId::RowCursorDisarm,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the detail chain"
                );
            }
            let toggle = keymap
                .first_binding_display(ActionId::TaskToggleTracker, &chain)
                .expect("the kind toggle resolves");
            let smart = keymap
                .first_binding_display(ActionId::TaskTrackToggle, &chain)
                .expect("the smart toggle resolves");
            assert_ne!(
                toggle, smart,
                "{preset:?}: the kind toggle and the smart toggle share a binding"
            );
        }
    }

    /// US-7 (W-8, #1032): the row-menu opener resolves in the Board
    /// and Today chains of every preset — the O-8 parity law, no
    /// second-class card surface.
    #[test]
    fn every_preset_resolves_the_row_menu_in_the_board_and_today_chains() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for context in [ContextId::Board, ContextId::Today] {
                let chain = [context, ContextId::Global];
                assert!(
                    keymap
                        .first_binding_display(ActionId::RowMenu, &chain)
                        .is_some(),
                    "{preset:?} does not resolve row.menu in the {context} chain"
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
            // The retune (J3 US-1, round 9 chunk 9): the item walk's
            // core carrier is nav.focus-down/up (the walk semantics
            // unchanged — the interpretation table's up/down bodies
            // are the FocusNext/Prev arms); the first/last jumps keep
            // their ids.
            ActionId::NavFocusDown,
            ActionId::NavFocusUp,
            ActionId::NavNextView,
            ActionId::NavPrevView,
            ActionId::BoardNext,
            ActionId::BoardPrevious,
            ActionId::StackWider,
            ActionId::StackNarrower,
            ActionId::TaskCreate,
            ActionId::StackCreate,
            ActionId::TaskEdit,
            ActionId::TaskComplete,
            ActionId::TaskMove,
            ActionId::TaskDelete,
            ActionId::MenuNext,
            ActionId::MenuPrevious,
            ActionId::MenuInvoke,
            ActionId::MenuDismiss,
            // Round-1 chunk 3 (D28b): the settings opener ships bound
            // in every preset (global) and closes via Escape in the
            // Settings context.
            ActionId::AppSettings,
            // Round 2 (D16–D21): every preset binds every new action
            // (the R3 no-second-class-preset gate).
            ActionId::TaskMoveCancel,
            ActionId::TaskReopen,
            // The stack step's carrier (J3 US-1, round 9 chunk 9):
            // nav.focus-right/left are the board's horizontal walk in
            // every preset (emacs' directional re-point; standard's
            // arrows; vi's h/l) — the StackNext/Prev ids stay bound
            // in standard/vi and menu-reachable everywhere.
            ActionId::NavFocusRight,
            ActionId::NavFocusLeft,
            ActionId::StackDelete,
            ActionId::StackRename,
            ActionId::StackMove,
            ActionId::BoardCreate,
            ActionId::BoardDelete,
            ActionId::BoardRename,
            ActionId::NavFirstItem,
            ActionId::NavLastItem,
            // Round 3 (D25): the control walk is bound in every
            // preset (the no-second-class gate).
            ActionId::NavNextControl,
            ActionId::NavPrevControl,
            // Round 3 (D26): the scroll actions too (the same gate;
            // the picker family stays out — uniformly bound, and
            // nav.show-task stays out — uniformly unbound).
            ActionId::ViewScrollUp,
            ActionId::ViewScrollDown,
            // Round 7 (D-7.2): the habit create opener joins the core
            // set — the accessibility ledger's parity demand makes it
            // bound in every preset (the no-second-class gate now
            // enforces that forever).
            ActionId::HabitCreate,
            // Chunk 8 (D4/D7, #934): the keyboard reorder pair and
            // the arm/undo keys join the core set — bound in every
            // preset (the same no-second-class gate).
            ActionId::HabitMoveUp,
            ActionId::HabitMoveDown,
            ActionId::HabitConvertArm,
            ActionId::HabitUndoLog,
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

    /// T-22 (#805): every preset binds the alarm window's total
    /// grammar — the three verbs over the selected row, the cursor
    /// walk, and the Escape close — in the `AlarmWindow` context (the
    /// contract's totality oracle; the window's own `FocusScope`
    /// handles the same keys).
    #[test]
    fn every_preset_binds_the_alarm_window_grammar() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            // The total mask: AlarmWindow masks the chain below it.
            assert!(
                preset.masks().contains(&ContextId::AlarmWindow),
                "{preset:?} does not mask the alarm window context"
            );
            let keymap = Keymap::from_preset(preset);
            let bindings = keymap
                .bindings_in(ContextId::AlarmWindow)
                .expect("the alarm window context is bound");
            let verbs: std::collections::BTreeSet<&ActionId> = bindings.values().collect();
            for verb in [
                ActionId::AlarmDismiss,
                ActionId::AlarmSnooze,
                ActionId::AlarmCancel,
                ActionId::MenuNext,
                ActionId::MenuPrevious,
                ActionId::MenuDismiss,
            ] {
                assert!(
                    verbs.contains(&verb),
                    "{preset:?} does not bind {verb} in the alarm window"
                );
            }
        }
    }

    /// US-6/AC-4 (the focus-restoration plan, D-N1): every preset
    /// binds the notification surface's engagement grammar — the walk
    /// pair, the Escape close, and the seven per-verb ids — over the
    /// `NotificationSurface` context (the r5 alarm contract's twin;
    /// the totality oracle; posture-independent).
    #[test]
    fn every_preset_binds_the_notification_surface_grammar() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            let bindings = keymap
                .bindings_in(ContextId::NotificationSurface)
                .expect("the notification surface context is bound");
            let verbs: std::collections::BTreeSet<&ActionId> = bindings.values().collect();
            for verb in [
                ActionId::MenuNext,
                ActionId::MenuPrevious,
                ActionId::MenuDismiss,
                ActionId::NotificationDismiss,
                ActionId::NotificationDone,
                ActionId::NotificationSnooze,
                ActionId::NotificationMute,
                ActionId::NotificationCancel,
                ActionId::NotificationTrackStart,
                ActionId::NotificationOpen,
            ] {
                assert!(
                    verbs.contains(&verb),
                    "{preset:?} does not bind {verb} in the notification surface"
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

    /// Round-1 chunk 3 (D28b): every preset resolves the settings
    /// opener in the settings surface's own chain — the Escape binding
    /// in `Settings` closes it; the opener rows stay global.
    #[test]
    fn every_preset_resolves_the_settings_close_in_the_settings_chain() {
        let chain = [ContextId::Settings, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            assert!(
                keymap
                    .first_binding_display(ActionId::AppSettings, &chain)
                    .is_some(),
                "{preset:?} does not resolve the settings close in the settings chain"
            );
        }
    }

    /// Round 2 (D16, the dispatch (b) contract): every preset resolves
    /// the move-cancel verb over the board chain — standard/emacs via
    /// Global Escape (emacs additionally via Board C-g), vi via the
    /// `ViNormal` Escape (the §3.4 carve-out).
    #[test]
    fn every_preset_resolves_the_move_cancel_in_the_board_chain() {
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            assert!(
                keymap
                    .first_binding_display(ActionId::TaskMoveCancel, &chain)
                    .is_some(),
                "{preset:?} does not resolve task.move.cancel in the board chain"
            );
        }
    }

    /// Round 2 (D17/D18): the stack-verb family mirrors the
    /// board-verb contract shape — every preset resolves each verb
    /// over the `[Board, ViNormal, Global]` chain.
    #[test]
    fn every_preset_resolves_the_stack_verbs_in_the_board_chain() {
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            // The stack STEP's carrier is per preset (J3 US-1,
            // round 9 chunk 9): emacs' directional rows carry it
            // (nav.focus-right/left — the board's horizontal body is
            // the step_stack walk); standard/vi keep the ids. The
            // lifecycle verbs stay uniform.
            let (step_next, step_prev) = match preset {
                Preset::Emacs => (ActionId::NavFocusRight, ActionId::NavFocusLeft),
                _ => (ActionId::StackNext, ActionId::StackPrev),
            };
            for verb in [
                step_next,
                step_prev,
                ActionId::StackDelete,
                ActionId::StackRename,
                ActionId::StackMove,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the board chain"
                );
            }
        }
    }

    /// Round 2 (D18): the board-lifecycle verbs resolve over the
    /// board chain in every preset (the same contract shape).
    #[test]
    fn every_preset_resolves_the_board_lifecycle_verbs_in_the_board_chain() {
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for verb in [
                ActionId::BoardCreate,
                ActionId::BoardDelete,
                ActionId::BoardRename,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the board chain"
                );
            }
        }
    }

    /// Round 2 (D16, the dispatch (c) C-g pin): under emacs the lone
    /// `Ctrl+g` cancels in the board chain (exact-match in Board),
    /// while everywhere the Board context is absent it stays a
    /// pending prefix of `app.redo` — `Ctrl+g Ctrl+/` still resolves
    /// the redo action. The exact-match-first resolver makes a
    /// Global C-g impossible without killing redo; this pins the
    /// recorded resolution.
    #[test]
    fn emacs_c_g_cancels_on_the_board_chain_and_stays_redos_prefix_elsewhere() {
        let keymap = Keymap::from_preset(Preset::Emacs);
        let board_chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        let task_chain = [
            ContextId::TaskDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        let c_g = stroke::parse("Ctrl+g").expect("ctrl g");
        let c_slash = stroke::parse("Ctrl+/").expect("ctrl slash");

        let mut resolver = crate::resolver::KeymapResolver::new();
        assert_eq!(
            resolver.feed(c_g, &board_chain, &keymap),
            crate::resolver::Resolution::Action(ActionId::TaskMoveCancel)
        );

        let mut resolver = crate::resolver::KeymapResolver::new();
        assert_eq!(
            resolver.feed(c_g, &task_chain, &keymap),
            crate::resolver::Resolution::Pending
        );
        assert_eq!(
            resolver.feed(c_slash, &task_chain, &keymap),
            crate::resolver::Resolution::Action(ActionId::AppRedo)
        );
    }

    /// The retune (J3 US-1, round 9 chunk 9, 2026-11-19): all four
    /// emacs directional chords carry nav.focus-* in every view
    /// context — the board's up/down stay the card walk, the board's
    /// left/right stay the stack step, the non-board horizontals and
    /// Settings' full set read as the control walk, the detail's keys
    /// never touch the subject (J3 R-3, the census-dated
    /// supersession). The up/down semantics are the old item walks
    /// under the new ids (the interpretation table's bodies are the
    /// FocusNext/Prev arms).
    #[test]
    fn emacs_four_chords_walk_controls_on_the_detail_chain() {
        let keymap = Keymap::from_preset(Preset::Emacs);
        let board_chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        let detail_chain = [
            ContextId::TaskDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        let today_chain = [ContextId::Today, ContextId::ViNormal, ContextId::Global];
        let stack_chain = [
            ContextId::StackDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        let settings_chain = [ContextId::Settings, ContextId::ViNormal, ContextId::Global];
        // The detail chain: all four chords are the nav.focus ids
        // (the control walk — the subject never moves).
        let detail_rows = [
            ("Ctrl+f", ActionId::NavFocusRight),
            ("Ctrl+b", ActionId::NavFocusLeft),
            ("Ctrl+n", ActionId::NavFocusDown),
            ("Ctrl+p", ActionId::NavFocusUp),
        ];
        // The board chain: up/down stay the card walk, left/right the
        // stack step — under the nav.focus ids.
        let board_rows = [
            ("Ctrl+n", ActionId::NavFocusDown),
            ("Ctrl+p", ActionId::NavFocusUp),
            ("Ctrl+f", ActionId::NavFocusRight),
            ("Ctrl+b", ActionId::NavFocusLeft),
        ];
        // Today/StackDetail: the item walks under nav.focus-down/up.
        let walk_rows = [
            ("Ctrl+n", ActionId::NavFocusDown),
            ("Ctrl+p", ActionId::NavFocusUp),
        ];
        for (spelling, action) in detail_rows {
            let stroke = stroke::parse(spelling).expect("the chord parses");
            assert_eq!(
                keymap.resolve(&detail_chain, &[stroke]),
                Some(action),
                "{spelling} carries the detail's nav.focus row"
            );
        }
        for (spelling, action) in board_rows {
            let stroke = stroke::parse(spelling).expect("the chord parses");
            assert_eq!(
                keymap.resolve(&board_chain, &[stroke]),
                Some(action),
                "{spelling} keeps the board's live walk"
            );
        }
        for (spelling, action) in walk_rows {
            let stroke = stroke::parse(spelling).expect("the chord parses");
            assert_eq!(
                keymap.resolve(&today_chain, &[stroke]),
                Some(action),
                "{spelling} keeps the today row walk"
            );
            assert_eq!(
                keymap.resolve(&stack_chain, &[stroke]),
                Some(action),
                "{spelling} keeps the stack list walk"
            );
        }
        // Settings resolves the control walk on all four chords
        // (C-n/C-p kept their r4 D19 Settings rows — the move is
        // TaskDetail-scoped; the US-1 AC-2 Settings evidence rides
        // these rows).
        let settings_rows = [
            ("Ctrl+f", ActionId::NavFocusRight),
            ("Ctrl+b", ActionId::NavFocusLeft),
            ("Ctrl+n", ActionId::NavFocusDown),
            ("Ctrl+p", ActionId::NavFocusUp),
        ];
        for (spelling, walk) in settings_rows {
            let stroke = stroke::parse(spelling).expect("the chord parses");
            assert_eq!(
                keymap.resolve(&settings_chain, &[stroke]),
                Some(walk),
                "{spelling} walks controls on the settings chain"
            );
        }
    }

    /// Round 3 (D25): the picker context is every preset's chain head
    /// while the grid holds focus, resolves the cursor verbs, and
    /// never masks — Escape still resolves `ModeNormal` from
    /// `ViInsert` (the editor's clean close), and the modal-masks
    /// contract is untouched.
    #[test]
    fn picker_context_chains_and_never_masks() {
        let escape = stroke::parse("Escape").expect("escape");
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            let chain = [
                ContextId::DatePicker,
                ContextId::ViInsert,
                ContextId::Global,
            ];
            for verb in [
                ActionId::PickerLeft,
                ActionId::PickerRight,
                ActionId::PickerUp,
                ActionId::PickerDown,
                ActionId::PickerPrevMonth,
                ActionId::PickerNextMonth,
                ActionId::PickerMonthStart,
                ActionId::PickerMonthEnd,
                ActionId::PickerCommit,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb} in the picker chain"
                );
            }
            assert_eq!(
                keymap.resolve(&chain, &[escape]),
                Some(ActionId::ModeNormal),
                "{preset:?}: Escape still closes the editor cleanly"
            );
            assert!(
                !keymap.visible_chain(&chain).is_empty()
                    && keymap
                        .visible_chain(&chain)
                        .last()
                        .is_some_and(|c| *c == ContextId::Global),
                "{preset:?}: the DatePicker context never masks"
            );
        }
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            assert!(!preset.masks().contains(&ContextId::DatePicker));
        }
    }

    /// Round 3 (D28): the emacs double-C-g convention — `C-g` parks
    /// as a pending prefix on the task chain, `C-g C-g` exactly
    /// resolves the move-cancel, and `C-g Ctrl+/` still resolves the
    /// redo; over the board chain the lone `C-g` still cancels
    /// instantly (exact-match-first).
    #[test]
    fn emacs_double_c_g_cancels_and_keeps_redo() {
        let keymap = Keymap::from_preset(Preset::Emacs);
        let task_chain = [
            ContextId::TaskDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        let board_chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        let c_g = stroke::parse("Ctrl+g").expect("ctrl g");
        let c_slash = stroke::parse("Ctrl+/").expect("ctrl slash");

        let mut resolver = crate::resolver::KeymapResolver::new();
        assert_eq!(
            resolver.feed(c_g, &task_chain, &keymap),
            crate::resolver::Resolution::Pending,
            "the first C-g parks (a prefix of both C-g C-g and C-g C-/)"
        );
        assert_eq!(
            resolver.feed(c_g, &task_chain, &keymap),
            crate::resolver::Resolution::Action(ActionId::TaskMoveCancel),
            "the second C-g cancels"
        );

        let mut resolver = crate::resolver::KeymapResolver::new();
        resolver.feed(c_g, &task_chain, &keymap);
        assert_eq!(
            resolver.feed(c_slash, &task_chain, &keymap),
            crate::resolver::Resolution::Action(ActionId::AppRedo),
            "C-g Ctrl+/ is still the redo"
        );

        let mut resolver = crate::resolver::KeymapResolver::new();
        assert_eq!(
            resolver.feed(c_g, &board_chain, &keymap),
            crate::resolver::Resolution::Action(ActionId::TaskMoveCancel),
            "the lone C-g still cancels on the board chain"
        );
    }

    /// Round 3 (D29, #231): the snooze/mute verbs are key-reachable in
    /// every preset (the no-second-class gate), and vi's Global stays
    /// exactly one row (the vi-shape contract's continuity).
    #[test]
    fn snooze_and_mute_are_reachable_in_every_preset() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            // vi's verbs ride ViNormal (the modal home); the session
            // chain always carries the mode context.
            let chain = if preset == Preset::Vi {
                vec![ContextId::ViNormal, ContextId::Global]
            } else {
                vec![ContextId::Global]
            };
            for verb in [ActionId::ReminderSnooze, ActionId::ReminderMute] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} does not resolve {verb}"
                );
            }
        }
        let vi = Keymap::from_preset(Preset::Vi);
        // J3 US-4 (round 9 chunk 11): the vi Global set grew to the
        // today jump plus the board-return's Alt+Left alternate (the
        // D-table's Back ruling; the dated supersession of the
        // one-row pin).
        assert_eq!(vi.bindings_in(ContextId::Global).expect("rows").len(), 2);
    }

    /// The chord-shift normalization (keymap-parity plan D-3, N-17;
    /// made directional by the operator's Dvorak-International
    /// role-theft report — issue #855) is additive and exact-match
    /// wins: every pre-existing binding resolves identically (the
    /// emacs `Alt+<` row resolves its exact event ahead of any
    /// partner substitution), the substitution fires only when Shift
    /// is genuinely held (a bare Alt+, dispatches nothing), an
    /// unmatched glyph falls through unchanged, and the
    /// shift-distinct chord pairs (`Ctrl+Shift+Z` ≠ `Ctrl+Z`) stay
    /// distinct — letters have no partner.
    #[test]
    fn normalization_is_additive_and_exact_match_wins() {
        let keymap = Keymap::from_preset(Preset::Emacs);
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        // The exact event for the bound token resolves as before.
        let exact =
            Stroke::from_event('<', false, true, false, false).expect("the exact event converts");
        assert_eq!(
            keymap.resolve(&chain, &[exact]),
            Some(ActionId::NavFirstItem)
        );
        // The Dvorak-shaped event (base glyph under Alt+Shift) now
        // resolves the same row through the partner pass.
        let dvorak =
            Stroke::from_event(',', false, true, true, false).expect("the shifted event converts");
        assert_eq!(
            keymap.resolve(&chain, &[dvorak]),
            Some(ActionId::NavFirstItem)
        );
        // The operator's role-theft report (issue #855, 2026-11-18):
        // a bare Alt+, — Shift NOT held — dispatches nothing. The
        // pre-fix symmetric map resolved it onto the begin token.
        let theft =
            Stroke::from_event(',', false, true, false, false).expect("the bare event converts");
        assert_eq!(
            keymap.resolve(&chain, &[theft]),
            None,
            "bare Alt+, must never resolve the Alt+< token"
        );
        // A layout that composes the `<` glyph with the Shift flag
        // still held resolves the same row through the fold candidate.
        let shifted_glyph =
            Stroke::from_event('<', false, true, true, false).expect("the glyph event converts");
        assert_eq!(
            keymap.resolve(&chain, &[shifted_glyph]),
            Some(ActionId::NavFirstItem)
        );
        // An unmatched glyph falls through unchanged.
        let unmatched = Stroke::from_event('%', false, true, true, false).expect("converts");
        assert_eq!(keymap.resolve(&chain, &[unmatched]), None);
        // Shift-distinct chords stay distinct: no partner on letters
        // (the undo binding's shifted-z identity vs the unbound
        // shift-less `Ctrl+z` — the re-homed redo lives on `Ctrl+y`).
        let standard = Keymap::from_preset(Preset::Standard);
        let global = [ContextId::Global];
        let undo = stroke::parse("Ctrl+Z").expect("undo (either spelling)");
        let redo = stroke::parse("Ctrl+y").expect("redo");
        let bare_undo = stroke::parse("Ctrl+z").expect("shift-less z");
        assert_eq!(standard.resolve(&global, &[undo]), Some(ActionId::AppUndo));
        assert_eq!(standard.resolve(&global, &[redo]), Some(ActionId::AppRedo));
        assert_eq!(standard.resolve(&global, &[bare_undo]), None);
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let before = Keymap::from_preset(preset);
            let built = build(&RawKeymap {
                preset,
                overrides: BTreeMap::new(),
            });
            assert_eq!(before, built.keymap, "{preset:?}: the map is untouched");
            assert_eq!(built.findings, [] as [Finding; 0]);
        }
    }

    /// The C-5 resolution pin (keymap-parity plan US-3 AC-1, D-5):
    /// both rows exist verbatim — the emacs scheduled-editor chord in
    /// the `TaskDetail` context and `ReminderSnooze`'s Global chord — and
    /// chain-order first-match decides: the detail chain resolves the
    /// scheduled editor (the view context precedes Global), every
    /// other chain resolves the snooze exactly as before.
    #[test]
    fn scheduled_edit_chord_wins_in_the_detail_chain_snooze_everywhere_else() {
        let keymap = Keymap::from_preset(Preset::Emacs);
        let chord = [
            stroke::parse("Ctrl+c").expect("c-c"),
            stroke::parse("Ctrl+s").expect("c-s"),
        ];
        let detail_chain = [
            ContextId::TaskDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        let board_chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        let today_chain = [ContextId::Today, ContextId::ViNormal, ContextId::Global];
        let settings_chain = [ContextId::Settings, ContextId::ViNormal, ContextId::Global];
        // Both rows verbatim (totality per context).
        assert_eq!(
            keymap.resolve(&detail_chain, &chord),
            Some(ActionId::TaskScheduledEdit)
        );
        for chain in [&board_chain, &today_chain, &settings_chain] {
            assert_eq!(
                keymap.resolve(chain, &chord),
                Some(ActionId::ReminderSnooze),
                "the snooze chord survives outside the detail chain"
            );
        }
    }

    /// US-3 AC-4 (keymap-parity plan, D-4): every preset binds the
    /// three new detail-editor ids in the `TaskDetail` context and the
    /// tracking pair in `TaskDetail` *and* `Today` — the generated hints
    /// resolve over the session chains (the no-second-class gate over
    /// the new ids).
    #[test]
    fn every_preset_binds_the_detail_shortcut_ids() {
        let detail_chain = [
            ContextId::TaskDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        let today_chain = [ContextId::Today, ContextId::ViNormal, ContextId::Global];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for verb in [
                ActionId::TaskDueEdit,
                ActionId::TaskScheduledEdit,
                ActionId::TaskDescriptionEdit,
                ActionId::ReminderRuleAdd,
                ActionId::TaskTrackStart,
                ActionId::TaskTrackStop,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &detail_chain).is_some(),
                    "{preset:?} does not bind {verb} in the detail chain"
                );
            }
            for verb in [ActionId::TaskTrackStart, ActionId::TaskTrackStop] {
                assert!(
                    keymap.first_binding_display(verb, &today_chain).is_some(),
                    "{preset:?} does not bind {verb} in the today chain"
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
            grouped.entry(context).or_default().push((action, seq));
        }
        grouped
    }

    /// T7 — intra-context conflict-freeness (D21): no two actions
    /// share a sequence in one context — post-normalization
    /// identities, since `parse_sequence` already applies the
    /// chorded-letter case law (J3 R-6). Replaces the compiler-side
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

    /// US-3/S2 — the voided pair (round plan O-8): under the binding
    /// identity the old deliberate `Ctrl+Z`/`Ctrl+Shift+Z` undo/redo
    /// distinction collapsed (`parse("Ctrl+Z")` ≡
    /// `parse("Ctrl+Shift+z")` — one identity), so the Standard redo
    /// was re-homed to the physically-deliverable `Ctrl+y` while undo
    /// keeps the shifted `z` identity. The spellings prove the
    /// collapse; the resolves prove the re-home landed and the
    /// shift-less `Ctrl+z` stays a distinct, unbound binding.
    #[test]
    fn us3_s2_undo_redo_pair_collapsed_and_the_redo_was_re_homed() {
        assert_eq!(
            stroke::parse("Ctrl+Z").expect("undo alias"),
            stroke::parse("Ctrl+Shift+z").expect("canonical undo")
        );
        let keymap = Keymap::from_preset(Preset::Standard);
        let global = [ContextId::Global];
        assert_eq!(
            keymap.resolve(&global, &[stroke::parse("Ctrl+Z").expect("undo press")]),
            Some(ActionId::AppUndo),
            "both undo spellings resolve the re-spelled shifted-z identity"
        );
        assert_eq!(
            keymap.resolve(&global, &[stroke::parse("Ctrl+y").expect("redo press")]),
            Some(ActionId::AppRedo),
            "the re-homed redo resolves"
        );
        assert_eq!(
            keymap.resolve(&global, &[stroke::parse("Ctrl+z").expect("bare ctrl z")]),
            None,
            "Ctrl+z stays distinct from the undo identity"
        );
    }

    /// US-3/AC-3 — the canonical-spelling fold (D4): every preset
    /// spelling of a chorded letter is the canonical form — mods in
    /// Ctrl/Alt/Shift/Meta order plus one lowercase letter — so no
    /// row spells a chorded uppercase letter (with or without
    /// Shift). `parse` still accepts the uppercase alias (T1), but
    /// the tables carry only canonical text.
    #[test]
    fn preset_chorded_letter_rows_spell_canonically() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            for (_, _, spellings) in preset.rows() {
                for spelling in spellings {
                    let stroke = stroke::parse(spelling).expect("preset spelling parses");
                    let chorded = stroke.ctrl || stroke.alt || stroke.meta;
                    if chorded && stroke.named_name().is_none() && stroke.key.is_ascii_alphabetic()
                    {
                        assert_eq!(
                            stroke.to_string(),
                            *spelling,
                            "{preset:?}: non-canonical chorded-letter spelling"
                        );
                    }
                }
            }
        }
    }

    /// US-3/S2 — the `from_event` delivery contract (the regression
    /// bar; no keymap test previously crossed the event boundary, so
    /// the parse∘resolve round trip never caught the case seam): for
    /// every preset row's chorded-letter stroke, the *real* keyboard
    /// deliveries resolve the row's action — for a shift-bearing
    /// identity both the lower-glyph and upper-glyph shifted
    /// deliveries, for a shift-free one the unshifted delivery — and
    /// the opposite shift reading never resolves it (`Ctrl+t` ≠
    /// `Ctrl+Shift+t`).
    #[test]
    fn us3_s2_from_event_deliveries_resolve_every_chorded_letter_row() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for (action, context, spellings) in preset.rows() {
                let chain = [context];
                for (i, spelling) in spellings.iter().enumerate() {
                    let token = stroke::parse(spelling).expect("preset spelling parses");
                    let chorded = token.ctrl || token.alt || token.meta;
                    // Bare printables are case-carrying by the D6
                    // fold; named keys carry no case — both are
                    // outside the chorded-letter law. Only the
                    // sequence's last stroke is a delivery — the
                    // earlier strokes are the held prefix.
                    if !chorded
                        || token.named_name().is_some()
                        || !token.key.is_ascii_alphabetic()
                        || i != spellings.len() - 1
                    {
                        continue;
                    }
                    let prefix: Vec<Stroke> = spellings[..i]
                        .iter()
                        .map(|s| stroke::parse(s).expect("prefix stroke parses"))
                        .collect();
                    // Chorded letter (reached when ctrl/alt/meta held).
                    let shifted_delivery = |glyph: char| {
                        Stroke::from_event(glyph, token.ctrl, token.alt, true, token.meta)
                            .expect("the shifted delivery converts")
                    };
                    let unshifted_delivery = Stroke::from_event(
                        token.key.to_ascii_lowercase(),
                        token.ctrl,
                        token.alt,
                        false,
                        token.meta,
                    )
                    .expect("the unshifted delivery converts");
                    let resolves = |seq: Vec<Stroke>| keymap.resolve(&chain, &seq) == Some(action);
                    let with_prefix = |last: Stroke| {
                        let mut seq = prefix.clone();
                        seq.push(last);
                        seq
                    };
                    if token.shift {
                        assert!(
                            resolves(with_prefix(shifted_delivery(token.key))),
                            "{preset:?} {action} in {context} stroke {i}: the shifted lower-glyph delivery resolves"
                        );
                        let upper = token.key.to_ascii_uppercase();
                        assert!(
                            resolves(with_prefix(shifted_delivery(upper))),
                            "{preset:?} {action} in {context} stroke {i}: the upper-glyph delivery is the same identity"
                        );
                        assert!(
                            !resolves(with_prefix(unshifted_delivery)),
                            "{preset:?} {action} in {context} stroke {i}: the shift-less delivery must not fire the shifted binding"
                        );
                    } else {
                        assert!(
                            resolves(with_prefix(unshifted_delivery)),
                            "{preset:?} {action} in {context} stroke {i}: the unshifted delivery resolves"
                        );
                        assert!(
                            !resolves(with_prefix(shifted_delivery(token.key))),
                            "{preset:?} {action} in {context} stroke {i}: the shifted delivery must not fire the shift-less binding"
                        );
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
    /// stays near-empty, `ViInsert` binds only the return-to-normal
    /// flip. (Chunk 10 D11 put the one-stroke today jump on Global in
    /// every preset — the deliberate, design-ordered exception; this
    /// test pins the Global set, not as "no Global". The round-9
    /// chunk-11 Back ruling added the board-return's alternate as the
    /// second row — dated in the assertion below.)
    #[test]
    fn vi_preset_shape_matches_the_mode_model() {
        let by_context = rows_by_context(Preset::Vi);
        let global = by_context.get(&ContextId::Global).expect("the global rows");
        // J3 US-4 (round 9 chunk 11, 2026-11-19): the Global row set
        // grows to two — the today jump plus the board-return's
        // Alt+Left alternate (the D-table's Back ruling; the dated
        // supersession of the one-row pin).
        assert_eq!(
            global.len(),
            2,
            "Global carries the today jump + the back alternate"
        );
        assert_eq!(global[0].0, ActionId::ViewToday);
        assert_eq!(global[1].0, ActionId::NavFocusBoard);
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
        // Round 2 (§3.4 carve-out): the all-preset cancel verb resolves
        // over the `[ViNormal, Global]` chain even with vi's Global
        // empty — the ViNormal Escape carries it.
        let keymap = Keymap::from_preset(Preset::Vi);
        assert!(
            keymap
                .first_binding_display(
                    ActionId::TaskMoveCancel,
                    &[ContextId::ViNormal, ContextId::Global]
                )
                .is_some(),
            "vi does not resolve task.move.cancel over the ViNormal chain"
        );
    }

    /// Masking declarations (D8/D11): every preset masks below the two
    /// modal surfaces — the menu and the search palette (#348) —
    /// `ViInsert` shadows without masking.
    #[test]
    fn presets_mask_only_the_modal_contexts() {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let masks: Vec<_> = preset.masks().to_vec();
            assert_eq!(
                masks,
                vec![
                    ContextId::Menu,
                    ContextId::Palette,
                    ContextId::FocusMode,
                    ContextId::TrackEntries,
                    ContextId::Labels,
                    ContextId::ReviewWalk,
                    ContextId::RecurEditor,
                    // The export dialog (the J2 export plan W-1): the
                    // adapter-owned VM's modal context (the trap-c
                    // sibling posture — the newest modal masks like
                    // its peers).
                    ContextId::ExportDialog,
                    // The alarm window (T-22, #805): the newest modal
                    // surface masks like its peers.
                    ContextId::AlarmWindow,
                ],
                "every modal surface masks; nothing else does"
            );
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
        assert_eq!(seq, vec!["Ctrl+Shift+q".to_owned()]);

        // An action bound nowhere in the chain is still `None`.
        assert_eq!(
            keymap.first_binding_display(ActionId::MenuNext, &session_chain),
            None
        );
    }
    /// The task-detail verb-table fold test (J3 US-4, round 9 chunk
    /// 11 — the stage-1 plan §3.4 law): every non-exempt row of the
    /// 22-verb table resolves a binding at `TaskDetail` in every preset
    /// — zero *regular* activatable detail widgets stay Tab-only. The
    /// plan's exempt rulings (chip unassign = pointer/Tab-indexed,
    /// rule-list verbs until chunk 12, series pause = Tab with the
    /// backlog candidate, the conversion family = rare-verb carve-out,
    /// `TaskToggleTracker` = menu-only) are deliberately NOT asserted —
    /// they are the recorded exceptions, not coverage misses.
    #[test]
    fn the_detail_verb_table_folds_to_a_binding_in_every_preset() {
        let chain = [
            ContextId::TaskDetail,
            ContextId::ViNormal,
            ContextId::Global,
        ];
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for verb in [
                ActionId::TaskComplete,
                ActionId::TaskReopen,
                ActionId::TaskTitleEdit,
                ActionId::TaskTrackToggle,
                ActionId::TaskTrackEntries,
                ActionId::TaskEffortEdit,
                ActionId::TaskEffortClear,
            ] {
                assert!(
                    keymap.first_binding_display(verb, &chain).is_some(),
                    "{preset:?} leaves {verb:?} Tab-only at the task detail"
                );
            }
        }
    }

    /// The R-2 mis-stroke contract (round 9 chunk 11): the emacs bare
    /// second stroke `C-c i` resolves the smart toggle while the
    /// chorded `C-c C-i` resolves the start verb — distinct bindings
    /// by the bare/chord rule (the `C-c c` capture precedent), at the
    /// detail AND on the board (where US-5 put the pair).
    #[test]
    fn the_emacs_toggle_and_start_strokes_stay_distinct() {
        use crate::keymap::stroke::parse;
        let keymap = Keymap::from_preset(Preset::Emacs);
        let bare = [parse("Ctrl+c").expect("Ctrl+c"), parse("i").expect("i")];
        let chorded = [
            parse("Ctrl+c").expect("Ctrl+c"),
            parse("Ctrl+i").expect("Ctrl+i"),
        ];
        for chain in [
            [
                ContextId::TaskDetail,
                ContextId::ViNormal,
                ContextId::Global,
            ],
            [ContextId::Board, ContextId::ViNormal, ContextId::Global],
        ] {
            assert_eq!(
                keymap.resolve(&chain, &bare),
                Some(ActionId::TaskTrackToggle),
                "bare C-c i is the toggle at {chain:?}"
            );
            assert_eq!(
                keymap.resolve(&chain, &chorded),
                Some(ActionId::TaskTrackStart),
                "chorded C-c C-i is the start at {chain:?}"
            );
        }
    }

    /// US-5's keymap-side contract (round 9 chunk 11): the start/stop
    /// twins resolve in the Board and `StackDetail` chains in every
    /// preset (the r8 rows' verbatim extension).
    #[test]
    fn the_tracker_pair_resolves_on_board_and_stack_detail_in_every_preset() {
        use crate::keymap::stroke::parse;
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let keymap = Keymap::from_preset(preset);
            for chain in [
                [ContextId::Board, ContextId::ViNormal, ContextId::Global],
                [
                    ContextId::StackDetail,
                    ContextId::ViNormal,
                    ContextId::Global,
                ],
            ] {
                let (start, stop) = match preset {
                    Preset::Standard => (parse("F9").expect("F9"), parse("F10").expect("F10")),
                    Preset::Emacs => (
                        parse("Ctrl+c").expect("Ctrl+c"),
                        parse("Ctrl+c").expect("Ctrl+c"),
                    ),
                    Preset::Vi => (parse("I").expect("I"), parse("O").expect("O")),
                };
                let start_seq: &[Stroke] = match preset {
                    Preset::Emacs => &[start, parse("Ctrl+i").expect("Ctrl+i")],
                    _ => &[start],
                };
                let stop_seq: &[Stroke] = match preset {
                    Preset::Emacs => &[stop, parse("Ctrl+o").expect("Ctrl+o")],
                    _ => &[stop],
                };
                assert_eq!(
                    keymap.resolve(&chain, start_seq),
                    Some(ActionId::TaskTrackStart),
                    "{preset:?} start resolves at {chain:?}"
                );
                assert_eq!(
                    keymap.resolve(&chain, stop_seq),
                    Some(ActionId::TaskTrackStop),
                    "{preset:?} stop resolves at {chain:?}"
                );
            }
        }
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
        // `Ctrl+J` (not the round-1 `Ctrl+Space` pick: the standard
        // preset binds it to `task.reopen` since round 2).
        let result = built(Preset::Standard, &[("task.complete", &[&["Ctrl+J"]])]);
        assert_eq!(result.findings, Vec::new());
        let space = stroke::parse("Space").expect("space");
        let override_seq = stroke::parse("Ctrl+J").expect("ctrl j");
        // The standard preset binds `task.complete` in the board
        // context (D8 draft); that is the context the override
        // replaces.
        let chain = [ContextId::Board, ContextId::Global];
        assert_eq!(
            result.keymap.resolve(&chain, &[override_seq]),
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
        // The exit binding's canonical identity (`Ctrl+Shift+q`) is
        // what the override collides with — both spellings mint it.
        let result = built(Preset::Standard, &[("app.undo", &[&["Ctrl+Q"]])]);
        let finding = one(&result, Conflict);
        assert_eq!(finding.action, Some(ActionId::AppUndo));
        assert_eq!(finding.context, ContextId::Global);
        let undo = stroke::parse("Ctrl+Z").expect("undo");
        let quit = stroke::parse("Ctrl+Shift+q").expect("quit");
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
        let redo = stroke::parse_sequence(["Ctrl+y"]).expect("redo");
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

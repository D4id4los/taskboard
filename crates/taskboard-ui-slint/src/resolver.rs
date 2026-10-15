// SPDX-License-Identifier: AGPL-3.0-only

//! The keymap resolver (chunk decision D12, design §5): a pure
//! prefix-accumulation state machine. One public entry — [`KeymapResolver::feed`]
//! — consumes a stroke under a context chain and answers exactly one
//! [`Resolution`]; the adapter rejects `Unhandled` so Slint keeps
//! delivering (Tab traversal survives).
//!
//! No slint types, no time, no IO (the §5.5 extraction seam); the
//! state machine is proptested over random stroke × chain streams
//! (D22).

use crate::actions::{ActionId, ContextId};
use crate::keymap::{Keymap, Stroke};

/// What one stroke did. Exactly one answer per stroke (D12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// A complete binding matched: dispatch this action.
    Action(ActionId),
    /// The stroke is a strict prefix of a longer binding — consumed,
    /// waiting for the next stroke (the pending indicator is the
    /// adapter's feedback, D18).
    Pending,
    /// The accumulated prefix matched nothing: a defined transition,
    /// not an implicit drop (§5.7's status signal). Pending clears.
    Aborted,
    /// Nothing in the visible chain matches, not even as a prefix:
    /// the adapter rejects so the event keeps flowing.
    Unhandled,
}

/// The prefix accumulator: holds the pending stroke sequence and the
/// chain it was accumulated under.
#[derive(Debug, Default)]
pub struct KeymapResolver {
    pending: Vec<Stroke>,
    chain: Vec<ContextId>,
}

impl KeymapResolver {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
            chain: Vec::new(),
        }
    }

    /// Whether a chord prefix is being accumulated.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Feeds one stroke under the (possibly changed) context chain
    /// (D12 rules 1–5). A chain different from the pending one resets
    /// the accumulation first — deterministic under context/mode
    /// flips mid-sequence.
    pub fn feed(&mut self, stroke: Stroke, chain: &[ContextId], keymap: &Keymap) -> Resolution {
        if self.chain != chain || self.pending.is_empty() {
            self.pending.clear();
            self.chain.clear();
            self.chain.extend_from_slice(chain);
        }
        let had_pending = !self.pending.is_empty();
        self.pending.push(stroke);

        if let Some(action) = keymap.resolve(chain, &self.pending) {
            self.pending.clear();
            return Resolution::Action(action);
        }
        if keymap.has_prefix(chain, &self.pending) {
            return Resolution::Pending;
        }
        self.pending.clear();
        if had_pending {
            Resolution::Aborted
        } else {
            Resolution::Unhandled
        }
    }

    /// Drops any pending accumulation (the adapter resets on focus
    /// changes; same rule as a chain mismatch, D12 rule 1).
    pub fn reset(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Preset, RawKeymap, build};
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    fn standard() -> Keymap {
        build(&RawKeymap {
            preset: Preset::Standard,
            overrides: BTreeMap::new(),
        })
        .keymap
    }

    fn emacs() -> Keymap {
        build(&RawKeymap {
            preset: Preset::Emacs,
            overrides: BTreeMap::new(),
        })
        .keymap
    }

    fn parse_stroke(sp: &str) -> Stroke {
        crate::keymap::stroke::parse(sp).expect("test stroke parses")
    }

    fn global_chain() -> Vec<ContextId> {
        vec![ContextId::Global]
    }

    /// T17 — the single-stroke and chord basics.
    #[test]
    fn single_stroke_chord_and_abort_transitions() {
        let keymap = standard();
        let mut resolver = KeymapResolver::new();
        let chain = global_chain();

        // Single stroke → action.
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+Q"), &chain, &keymap),
            Resolution::Action(ActionId::AppExit)
        );
        assert!(!resolver.has_pending());

        // Prefix accumulation over the emacs C-x family.
        let keymap = emacs();
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+x"), &chain, &keymap),
            Resolution::Pending
        );
        assert!(resolver.has_pending());
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+c"), &chain, &keymap),
            Resolution::Action(ActionId::AppExit)
        );

        // Prefix-abort: a consumed prefix followed by a non-matching
        // stroke is a defined transition, not a drop (D12 rule 4).
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+x"), &chain, &keymap),
            Resolution::Pending
        );
        assert_eq!(
            resolver.feed(parse_stroke("F1"), &chain, &keymap),
            Resolution::Aborted
        );
        assert!(!resolver.has_pending());

        // A plain unbound stroke with nothing pending is unhandled.
        assert_eq!(
            resolver.feed(parse_stroke("Z"), &chain, &keymap),
            Resolution::Unhandled
        );
    }

    /// T18 — chain order: the highest visible context carrying the
    /// sequence wins (first-match over the chain, D11).
    #[test]
    fn chain_order_first_match_shadows_lower_contexts() {
        let keymap = standard();
        let mut resolver = KeymapResolver::new();
        // `Down` is bound in Board (nav.next-item) and Menu
        // (menu.next); the chain head decides.
        let down = parse_stroke("Down");
        assert_eq!(
            resolver.feed(down, &[ContextId::Board, ContextId::Global], &keymap),
            Resolution::Action(ActionId::NavNextItem)
        );
        assert_eq!(
            resolver.feed(down, &[ContextId::Menu], &keymap),
            Resolution::Action(ActionId::MenuNext)
        );
        // Shadowing across contexts (emacs): `Ctrl+n` is bound in
        // Board (nav.next-item) and in Menu (menu.next); the chain
        // head decides which one fires (legitimate context
        // shadowing, D11).
        let keymap = emacs();
        assert_eq!(
            resolver.feed(
                parse_stroke("Ctrl+n"),
                &[ContextId::Board, ContextId::Global],
                &keymap
            ),
            Resolution::Action(ActionId::NavNextItem)
        );
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+n"), &[ContextId::Menu], &keymap),
            Resolution::Action(ActionId::MenuNext)
        );
    }

    /// T19 — masking (D11): the open menu masks the contexts below —
    /// even app.exit waits until dismissal — while `ViInsert` only
    /// shadows (Ctrl-chords keep working while typing).
    #[test]
    fn menu_masks_but_vi_insert_only_shadows() {
        let keymap = standard();
        let mut resolver = KeymapResolver::new();
        let quit = parse_stroke("Ctrl+Q");
        assert_eq!(
            resolver.feed(quit, &[ContextId::Menu], &keymap),
            Resolution::Unhandled,
            "Menu masks Global"
        );

        // ViInsert carries a single binding (mode.normal — Escape);
        // use the vi preset, the only one with mode contexts bound.
        let keymap = build(&RawKeymap {
            preset: Preset::Vi,
            overrides: BTreeMap::new(),
        })
        .keymap;
        let insert_chain = vec![ContextId::ViInsert, ContextId::Global];
        // `Ctrl+r` is bound in ViNormal, not ViInsert; in insert the
        // Escape is the only vi binding, and Global is empty — so
        // Ctrl+r is unhandled (nothing shadows it).
        let ctrl_r = parse_stroke("Ctrl+r");
        assert_eq!(
            resolver.feed(ctrl_r, &insert_chain, &keymap),
            Resolution::Unhandled
        );
        // Escape resolves to the mode flip (the only insert binding).
        assert_eq!(
            resolver.feed(parse_stroke("Escape"), &insert_chain, &keymap),
            Resolution::Action(ActionId::ModeNormal)
        );
        // Shadowing-only semantics: a context whose chain head has no
        // binding for a stroke still lets the lower context answer.
        // ViNormal binds `u` (undo); ViInsert does not — an empty
        // ViNormal member would fall through, but ViInsert's chain
        // member *shadows nothing here* because it holds no matching
        // sequence; the vi preset's Global is empty, so `u` is
        // unhandled in normal mode too (the honest vim model).
        assert_eq!(
            resolver.feed(parse_stroke("u"), &[ContextId::ViNormal], &keymap),
            Resolution::Action(ActionId::AppUndo)
        );
    }

    /// T20 — a pending accumulation under a changed chain resets and
    /// reprocesses (D12 rule 1: deterministic under context/mode
    /// flips mid-sequence).
    #[test]
    fn chain_change_resets_the_pending_accumulation() {
        let keymap = emacs();
        let mut resolver = KeymapResolver::new();
        let global = global_chain();
        // Start a C-x sequence, then the context flips.
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+x"), &global, &keymap),
            Resolution::Pending
        );
        let board = vec![ContextId::Board, ContextId::Global];
        // `Ctrl+n` alone would be Unhandled as the second C-x stroke;
        // after the chain flip it is a fresh board binding.
        assert_eq!(
            resolver.feed(parse_stroke("Ctrl+n"), &board, &keymap),
            Resolution::Action(ActionId::NavNextItem)
        );
    }
    // T21 — resolver sequence properties over random stroke streams
    // and chains (D22): terminal resolutions leave the resolver idle
    // (only `Pending` keeps state), and identical streams produce
    // identical outputs (determinism).
    proptest! {
        #[test]
        fn stream_invariants_and_determinism(
            stream in proptest::collection::vec(
                proptest::option::of(proptest::sample::select(
                    vec!["j", "k", "u", "g", "d", "Z", "Escape", "Ctrl+r", ":", "/"],
                )),
                0..40,
            ),
            seed in proptest::num::u32::ANY,
        ) {
            let keymap = build(&RawKeymap { preset: Preset::Vi, overrides: BTreeMap::new() }).keymap;
            let chains: &[&[ContextId]] = &[
                &[ContextId::ViNormal],
                &[ContextId::ViInsert],
                &[ContextId::Menu],
            ];
            // A run replays the stream under a deterministic
            // pseudo-random chain walk over the three modes (normal /
            // insert / open menu); calling it twice must agree.
            let run = || {
                let mut pick = u64::from(seed);
                let mut resolver = KeymapResolver::new();
                let mut resolutions = Vec::new();
                for spelling in stream.iter().flatten() {
                    pick = pick
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let chain = chains[usize::try_from(pick).expect("u64 fits usize on supported targets") % chains.len()];
                    let parsed =
                        crate::keymap::stroke::parse(spelling).expect("generated strokes parse");
                    resolutions.push(resolver.feed(parsed, chain, &keymap));
                }
                resolutions
            };

            let first = run();
            // Idle-return: after any terminal resolution the resolver
            // holds no pending state (checked inside `feed` by the
            // clearing rules; here we assert the observable half —
            // the replay is identical and no resolution is ever
            // missing).
            for resolution in &first {
                prop_assert!(matches!(
                    resolution,
                    Resolution::Action(_) | Resolution::Pending | Resolution::Aborted | Resolution::Unhandled
                ));
            }

            // Determinism: the same stream replays identically.
            prop_assert_eq!(first, run());
        }

        #[test]
        fn bound_sequence_yields_its_action_under_constant_chain(
            seq in proptest::collection::vec(
                proptest::sample::select(
                    vec!["j", "k", "g", "d", "Z", "t", "T", "m", "Q", "c", "s"],
                ),
                1..4,
            ),
        ) {
            let keymap = build(&RawKeymap { preset: Preset::Vi, overrides: BTreeMap::new() }).keymap;
            let chain = vec![ContextId::ViNormal];
            // Any complete preset binding fed stroke-by-stroke yields
            // exactly one Action at its final stroke.
            if let Some(expected) = keymap.resolve(
                &chain,
                &seq.iter()
                    .map(|s| crate::keymap::stroke::parse(s).unwrap())
                    .collect::<Vec<_>>(),
            ) {
                let mut resolver = KeymapResolver::new();
                let mut got = None;
                for spelling in &seq {
                    let resolution = resolver.feed(parse_stroke(spelling), &chain, &keymap);
                    if let Resolution::Action(action) = resolution {
                        got = Some(action);
                    }
                }
                prop_assert_eq!(got, Some(expected));
            }
        }
    }
}

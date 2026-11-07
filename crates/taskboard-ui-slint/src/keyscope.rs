// SPDX-License-Identifier: AGPL-3.0-only

//! The keybind context stack (round-5 chunk 7, D-KS1): the
//! declarative replacement for `KeymapShell::chain`'s boolean
//! reconstruction. A scope is a registration row evaluated (pulled)
//! at intercept time from already-mirrored state — no stored
//! registrations, so none can outlive its surface (the P5-class
//! stale-scope trap is avoided by construction: every arming
//! condition is a fact the adapter re-reads per keystroke).
//!
//! Ordering (round plan §4.7): focused-element scopes (innermost
//! first) → enclosing surface scopes (innermost first) → view → mode
//! → global. [`scope_stack`] is pure — no slint types; the adapter
//! passes the pulled facts in [`ScopeInputs`].

use crate::actions::ContextId;
use crate::shell::KeymapShell;
use crate::view_state::Flow;

/// The facts one intercept (or refresh pass) pulls for
/// [`scope_stack`]. Every field is already-mirrored state — nothing
/// here is stored between keystrokes (D-KS1's self-healing rule).
#[derive(Debug, Clone, Copy)]
pub struct ScopeInputs<'a> {
    /// The shell's mode/menu/modal mirrors (the legacy chain's
    /// inputs, verbatim).
    pub shell: &'a KeymapShell,
    /// The due/scheduled picker panel is mounted (`due_picker.is_some()`
    /// in the adapter). Together with the `Flow::EditDate` flow and a
    /// non-focused grid this arms the F11 field-phase head (D-δ1).
    pub picker_open: bool,
    /// `EditingFacts.editor-focus-count > 0`: the multi-line editor's
    /// element scope rides the head (D-δ2).
    pub editor_focus: bool,
    /// The view-state flow, pulled per intercept (the `EditDate`
    /// arming's other half).
    pub flow: &'a Flow,
    /// The pending stroke at the key doorway (`None` everywhere else —
    /// refresh-pass hint chains never arm the deltas).
    pub stroke: Option<crate::keymap::Stroke>,
    /// Whether the caller is the key doorway (`intercept`). The two
    /// deltas arm ONLY there: resolution happens at the doorway, so
    /// that is the only place a new context can change outcomes; the
    /// refresh-pass hint chains stay byte-legacy.
    pub at_key_doorway: bool,
}

impl ScopeInputs<'_> {
    /// D-δ1's arming predicate: the picker's FIELD phase — the
    /// `EditDate` flow holds, the picker panel is mounted, and the
    /// grid does NOT hold slint focus (the grid phase already rides
    /// the head through `date_picker_grid`).
    #[must_use]
    pub fn picker_field_phase(&self) -> bool {
        matches!(self.flow, Flow::EditDate { .. })
            && self.picker_open
            && !self.shell.date_picker_grid
    }

    /// D-δ1's ladder exemption: bare Return and bare Escape stay with
    /// the field. Both are pinned journeys
    /// (`the_due_picker_journeys_reach_the_boundary`: the field's
    /// Return commits the typed date; the field phase's Escape is the
    /// phase step back) — the field-phase head serves only the
    /// chord-class and month-navigation strokes, never these two.
    #[must_use]
    pub fn field_phase_head_exempt(&self) -> bool {
        let Some(stroke) = self.stroke else {
            return false;
        };
        !stroke.ctrl
            && !stroke.alt
            && !stroke.meta
            && !stroke.shift
            && matches!(stroke.named_name(), Some("Return" | "Escape"))
    }

    /// Whether the field-phase picker head arms under these facts.
    #[must_use]
    pub fn field_phase_head_arms(&self) -> bool {
        self.at_key_doorway && self.picker_field_phase() && !self.field_phase_head_exempt()
    }
}

/// The ordered context stack under the pulled facts. Byte-equal to
/// the retired `KeymapShell::chain` for every legacy state (the T-S1
/// port checklist pins all nine shapes); the two declared deltas are
/// the field-phase picker head (D-δ1) and the editor head (D-δ2).
#[must_use]
pub fn scope_stack(inputs: &ScopeInputs) -> Vec<ContextId> {
    let shell = inputs.shell;
    if shell.menu.open {
        return vec![ContextId::Menu];
    }
    if shell.palette {
        // The palette is the second modal context (D8): insert-mode
        // head so the field's printables are unbound everywhere, and
        // the `Palette` mask makes the chain total.
        return vec![ContextId::Palette, ContextId::ViInsert, ContextId::Global];
    }
    if shell.focus_overlay {
        // The concentration overlay is the third modal context
        // (#496): the same total-mask shape as the palette.
        return vec![ContextId::FocusMode, ContextId::ViInsert, ContextId::Global];
    }
    if shell.track_entries {
        // The time-entries overlay is the fourth modal context
        // (r4 chunk 10, D-TS2).
        return vec![
            ContextId::TrackEntries,
            ContextId::ViInsert,
            ContextId::Global,
        ];
    }
    if shell.review_walk {
        // The review walk is the fifth modal context (DR-7 chunk 3,
        // #638).
        return vec![
            ContextId::ReviewWalk,
            ContextId::ViInsert,
            ContextId::Global,
        ];
    }
    let mut chain = Vec::with_capacity(5);
    // D-δ2: the editor element scope rides the head (non-masking,
    // keymap-unbound — its verbs are the command tables, not rows).
    // Doorway-only: the refresh-pass hint chains stay byte-legacy.
    if inputs.editor_focus && inputs.at_key_doorway {
        chain.push(ContextId::Editor);
    }
    // The picker surface's scope: the grid phase (round-3 D25) and —
    // the F11 delta — the field phase (D-δ1), both non-masking. The
    // field-phase arm carries the Return/Escape exemption.
    if shell.date_picker_grid || inputs.field_phase_head_arms() {
        chain.push(ContextId::DatePicker);
    }
    if shell.mode == crate::router::Mode::Insert {
        chain.push(ContextId::ViInsert);
        chain.push(ContextId::Global);
        return chain;
    }
    if let Some(view) = shell.view {
        chain.push(view);
    }
    chain.push(ContextId::ViNormal);
    chain.push(ContextId::Global);
    chain
}

/// The printable monopoly (D-KS2): while a text field holds slint
/// focus, it claims every monopolizable stroke outright — `intercept`
/// rejects (returns `Unhandled`) before any scope or resolver step,
/// so the focused field receives the key. Gated on the insert mode:
/// in every flowless state the D20 pull makes the mode Insert exactly
/// while `text_focus > 0`, so the gate reproduces today's insert-mask
/// protection verbatim (an assertion in T-M1) while keeping the vi
/// preset's Normal-mode editor ladder reachable.
#[must_use]
pub fn monopoly_declines(
    mode: crate::router::Mode,
    text_focus: usize,
    stroke: crate::keymap::Stroke,
) -> bool {
    mode == crate::router::Mode::Insert
        && text_focus > 0
        && !stroke.ctrl
        && !stroke.alt
        && !stroke.meta
        && stroke.is_monopolizable()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ContextId;
    use crate::router::Mode;
    use crate::view_state::{DateField, Flow};
    use proptest::prelude::*;
    use taskboard_domain::TaskId;

    fn inputs<'a>(
        shell: &'a KeymapShell,
        flow: &'a Flow,
        picker_open: bool,
        editor_focus: bool,
    ) -> ScopeInputs<'a> {
        ScopeInputs {
            shell,
            picker_open,
            editor_focus,
            flow,
            stroke: None,
            at_key_doorway: false,
        }
    }

    fn doorway<'a>(
        shell: &'a KeymapShell,
        flow: &'a Flow,
        picker_open: bool,
        editor_focus: bool,
        stroke: Option<crate::keymap::Stroke>,
    ) -> ScopeInputs<'a> {
        ScopeInputs {
            at_key_doorway: true,
            stroke,
            ..inputs(shell, flow, picker_open, editor_focus)
        }
    }

    /// T-S1 — the port checklist (plan §4): every legacy chain shape
    /// byte-equal between `scope_stack` and the retired
    /// `KeymapShell::chain` (the deltas' arming facts off). The
    /// checklist's nine rows are the contract's length.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn the_port_checklist_matches_the_legacy_chain_byte_for_byte() {
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let no_flow = Flow::None;
        let edit_date = Flow::EditDate {
            task,
            field: DateField::Due,
        };

        // 1. menu open → [Menu].
        let shell = KeymapShell {
            menu: crate::shell::MenuState {
                open: true,
                ..crate::shell::MenuState::default()
            },
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(scope_stack(&i), vec![ContextId::Menu]);

        // 2–5. the modal surfaces: the flow-armed chains.
        let shell = KeymapShell {
            palette: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::Palette, ContextId::ViInsert, ContextId::Global]
        );

        let shell = KeymapShell {
            focus_overlay: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::FocusMode, ContextId::ViInsert, ContextId::Global]
        );

        let shell = KeymapShell {
            track_entries: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::TrackEntries,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );

        let shell = KeymapShell {
            review_walk: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::ReviewWalk,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );

        // 6. insert + grid focus → [DatePicker, ViInsert, Global].
        let shell = KeymapShell {
            mode: Mode::Insert,
            date_picker_grid: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &edit_date, true, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );

        // 7. insert, no grid → [ViInsert, Global] (the field phase's
        //    delta OFF: the legacy shape).
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &edit_date, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::ViInsert, ContextId::Global]
        );

        // 8. normal, grid focus, view Board → [DatePicker, Board,
        //    ViNormal, Global].
        let shell = KeymapShell {
            view: Some(ContextId::Board),
            date_picker_grid: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &edit_date, true, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::Board,
                ContextId::ViNormal,
                ContextId::Global
            ]
        );

        // 9. normal, view Board → [Board, ViNormal, Global].
        let shell = KeymapShell {
            view: Some(ContextId::Board),
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), shell.chain());
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::Board, ContextId::ViNormal, ContextId::Global]
        );
    }

    /// T-S1's delta rows: the two intentional behavioral diffs.
    #[test]
    fn the_declared_deltas_ride_the_head() {
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let edit_date = Flow::EditDate {
            task,
            field: DateField::Due,
        };

        // D-δ1: the picker's field phase arms the head non-masking —
        // `[DatePicker, ViInsert, Global]` where the legacy chain is
        // `[ViInsert, Global]` — at the key doorway, for chord-class
        // strokes only.
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let chord = crate::keymap::stroke::parse("Ctrl+p").expect("parses");
        let i = doorway(&shell, &edit_date, true, false, Some(chord));
        assert!(i.picker_field_phase());
        assert!(i.field_phase_head_arms());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );
        // The pinned exemption: bare Return/Escape keep the legacy
        // chain (the field's commit and the phase step-back).
        for spelling in ["Return", "Escape"] {
            let bare = crate::keymap::stroke::parse(spelling).expect("parses");
            let i = doorway(&shell, &edit_date, true, false, Some(bare));
            assert!(i.picker_field_phase());
            assert!(
                !i.field_phase_head_arms(),
                "{spelling} stays with the field"
            );
            assert_eq!(scope_stack(&i), shell.chain());
        }
        // Off-doorway callers (refresh-pass hints) never arm it.
        let i = inputs(&shell, &edit_date, true, false);
        assert!(i.picker_field_phase());
        assert!(!i.field_phase_head_arms());
        assert_eq!(scope_stack(&i), shell.chain());
        // The grid phase keeps the legacy head (no double ride).
        let shell = KeymapShell {
            mode: Mode::Insert,
            date_picker_grid: true,
            ..KeymapShell::default()
        };
        let i = doorway(&shell, &edit_date, true, false, Some(chord));
        assert!(!i.picker_field_phase());
        assert_eq!(scope_stack(&i), shell.chain());
        // A mounted picker without the EditDate flow arms nothing.
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let i = doorway(&shell, &Flow::None, true, false, Some(chord));
        assert!(!i.picker_field_phase());
        assert_eq!(scope_stack(&i), shell.chain());

        // D-δ2: the editor element scope rides the head — doorway only.
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let i = doorway(&shell, &Flow::None, false, true, Some(chord));
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::Editor, ContextId::ViInsert, ContextId::Global]
        );
        let i = inputs(&shell, &Flow::None, false, true);
        assert_eq!(scope_stack(&i), shell.chain(), "off-doorway stays legacy");
        // The Editor context is keymap-unbound in every preset and
        // never masks — the resolver's chain walk skips it.
        for preset in [
            crate::keymap::Preset::Standard,
            crate::keymap::Preset::Emacs,
            crate::keymap::Preset::Vi,
        ] {
            let keymap = crate::keymap::Keymap::from_preset(preset);
            assert!(!preset.masks().contains(&ContextId::Editor));
            assert!(keymap.bindings_in(ContextId::Editor).is_none());
        }
    }

    // T-S1's property arm: for arbitrary (modal, mode, grid, view,
    // picker, editor-focus) tuples, `scope_stack` equals the legacy
    // chain except exactly the two declared deltas.
    proptest! {
        #[test]
        fn scope_stack_equals_the_legacy_chain_except_the_declared_deltas(
            menu in proptest::bool::ANY,
            palette in proptest::bool::ANY,
            focus_overlay in proptest::bool::ANY,
            track_entries in proptest::bool::ANY,
            review_walk in proptest::bool::ANY,
            insert in proptest::bool::ANY,
            grid in proptest::bool::ANY,
            has_view in proptest::bool::ANY,
            picker_open in proptest::bool::ANY,
            editor_focus in proptest::bool::ANY,
            edit_date_flow in proptest::bool::ANY,
            at_doorway in proptest::bool::ANY,
            bare_return_escape in proptest::bool::ANY,
        ) {
            let shell = KeymapShell {
                menu: crate::shell::MenuState {
                    open: menu,
                    ..crate::shell::MenuState::default()
                },
                palette,
                focus_overlay,
                track_entries,
                review_walk,
                mode: if insert { Mode::Insert } else { Mode::Normal },
                date_picker_grid: grid,
                view: has_view.then_some(ContextId::Board),
                ..KeymapShell::default()
            };
            let task = TaskId::from_uuid(uuid::Uuid::new_v4());
            let flow = if edit_date_flow {
                Flow::EditDate { task, field: DateField::Due }
            } else {
                Flow::None
            };
            let stroke = bare_return_escape.then(|| {
                crate::keymap::stroke::parse("Return").expect("parses")
            });
            let i = ScopeInputs {
                at_key_doorway: at_doorway,
                stroke,
                ..inputs(&shell, &flow, picker_open, editor_focus)
            };
            let stack = scope_stack(&i);
            if !at_doorway
                || menu || palette || focus_overlay || track_entries || review_walk
            {
                // Off-doorway and the modal shapes are untouched by
                // both deltas.
                prop_assert_eq!(stack, shell.chain());
            } else {
                let mut legacy = shell.chain();
                if editor_focus {
                    legacy.insert(0, ContextId::Editor);
                }
                if i.field_phase_head_arms() {
                    let at = usize::from(editor_focus);
                    legacy.insert(at, ContextId::DatePicker);
                }
                prop_assert_eq!(stack, legacy);
            }
        }
    }
    /// T-M1 — the monopolized set: every bare printable and Space is
    /// the field's in insert mode; a chord is not; and the D20
    /// convergence: the mode gate makes the monopoly agree with the
    /// insert chain (a flowless field is Insert whenever the count is
    /// positive), while the Normal-mode gate keeps the vi editor
    /// ladder reachable.
    #[test]
    fn the_monopoly_claims_printables_and_space_in_insert() {
        let ok = |sp: &str| {
            let s = crate::keymap::stroke::parse(sp).expect("parses");
            assert!(s.is_monopolizable(), "{sp} is monopolizable");
            assert!(
                monopoly_declines(Mode::Insert, 1, s),
                "{sp} declines to the field"
            );
        };
        for sp in ["a", "Z", "9", "!", "Space"] {
            ok(sp);
        }
        // Chords never monopolize — even of a monopolizable key (the
        // rule gates on the modifiers, the predicate on the key).
        let chord = crate::keymap::stroke::parse("Ctrl+a").expect("parses");
        assert!(chord.is_monopolizable());
        assert!(!monopoly_declines(Mode::Insert, 1, chord));
        // No focus, no monopoly; no insert, no monopoly (the vi
        // Normal-mode ladder stays reachable).
        let a = crate::keymap::stroke::parse("a").expect("parses");
        assert!(!monopoly_declines(Mode::Insert, 0, a));
        assert!(!monopoly_declines(Mode::Normal, 1, a));
    }

    /// T-M2 — the chord-class set: every named non-Space key is
    /// scope-bindable (the monopoly never takes it), so Tab traversal,
    /// Return/Escape commit-discard, and vi insert's Escape survive
    /// every field.
    #[test]
    fn named_keys_stay_chord_class() {
        for sp in [
            "Return",
            "Escape",
            "Tab",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Up",
            "Down",
            "Left",
            "Right",
            "F1",
            "F12",
        ] {
            let s = crate::keymap::stroke::parse(sp).expect("parses");
            assert!(!s.is_monopolizable(), "{sp} stays chord-class");
            assert!(!monopoly_declines(Mode::Insert, 1, s));
        }
    }

    /// T-M3 — Backspace's fall-through: no preset binds Backspace
    /// anywhere, so the unmonopolized stroke resolves Unhandled on
    /// every chain and reaches the field natively — the same
    /// falls-through-to-native result as today.
    #[test]
    fn backspace_is_bound_nowhere_and_falls_through() {
        for preset in [
            crate::keymap::Preset::Standard,
            crate::keymap::Preset::Emacs,
            crate::keymap::Preset::Vi,
        ] {
            for (_, context, spellings) in preset.rows() {
                for sp in spellings.iter().copied() {
                    let parsed = crate::keymap::stroke::parse(sp).expect("preset rows parse");
                    assert_ne!(
                        parsed.named_name(),
                        Some("Backspace"),
                        "{preset:?} binds Backspace in {context}"
                    );
                }
            }
        }
    }

    /// T-M4 — the activation disjointness: the reservation
    /// (Return/Space for a focused activation cell) and the monopoly
    /// are disjoint by the facts — an activation stop holds
    /// `activation-focus-count > 0` with `text-focus-count == 0`, so
    /// the monopoly never fires on a reserved stroke. Pinned here on
    /// the pure rule: the reservation's Space is monopolizable ONLY
    /// when a text focus exists.
    #[test]
    fn the_reservation_and_the_monopoly_are_disjoint_by_the_facts() {
        let space = crate::keymap::stroke::parse("Space").expect("parses");
        let ret = crate::keymap::stroke::parse("Return").expect("parses");
        for stroke in [space, ret] {
            // The activation cell's fact state: zero text focus.
            assert!(!monopoly_declines(Mode::Insert, 0, stroke));
        }
        // And the reservation's own gate requires a bare stroke.
        let chord_space = crate::keymap::stroke::parse("Ctrl+Space").expect("parses");
        assert!(chord_space.ctrl);
    }
}

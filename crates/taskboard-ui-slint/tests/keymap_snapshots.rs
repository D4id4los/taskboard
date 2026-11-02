// SPDX-License-Identifier: AGPL-3.0-only

//! Keymap-layer insta snapshots — "documentation as a test" (design
//! §10, D20). The registry dump *is* the action documentation (§5.2);
//! the resolved preset keymaps are the preset documentation. Reviewed
//! by hand, never blindly accepted; regeneration is an explicit
//! `INSTA_UPDATE=always cargo nextest run -p taskboard-ui-slint` run
//! recorded in the commit message.

#![cfg_attr(miri, ignore)] // insta needs socketpair (environment gate)

use insta::assert_yaml_snapshot;
use serde::Serialize;

use taskboard_ui_slint::actions::REGISTRY;
use taskboard_ui_slint::keymap::{Keymap, Preset};

#[derive(Serialize)]
struct RegistryRow<'a> {
    id: &'a str,
    label: &'a str,
    contexts: Vec<&'a str>,
    category: &'a str,
    enabled: bool,
    menu_rank: u16,
    description: &'a str,
    menu_group: Option<&'static str>,
}

/// The action registry dump: one row per action, in registry order —
/// id, label, menu-availability contexts, category, the honest
/// `enabled` bit (§5.8: reserved actions read as disabled), and the
/// round-1 presentation columns (menu rank, description, menu group).
#[test]
fn registry_dump_is_the_action_documentation() {
    let rows: Vec<RegistryRow<'_>> = REGISTRY
        .iter()
        .map(|entry| RegistryRow {
            id: entry.id.id(),
            label: entry.label,
            contexts: entry.contexts.iter().map(|c| c.name()).collect(),
            category: entry.category.name(),
            enabled: entry.enabled,
            menu_rank: entry.menu_rank,
            description: entry.description,
            menu_group: entry.menu_group.map(crate_group_name),
        })
        .collect();
    assert_yaml_snapshot!("registry", rows);
}

/// The stable snapshot name of a menu group.
fn crate_group_name(group: taskboard_ui_slint::actions::MenuGroup) -> &'static str {
    use taskboard_ui_slint::actions::MenuGroup::*;
    match group {
        Task => "task",
        Stack => "stack",
        Board => "board",
        ViewNav => "view-nav",
        Mode => "mode",
        Help => "help",
    }
}

#[derive(Serialize)]
struct BindingRow {
    sequence: Vec<String>,
    action: String,
}

#[derive(Serialize)]
struct ContextRow {
    context: String,
    bindings: Vec<BindingRow>,
}

#[derive(Serialize)]
struct PresetRow {
    preset: String,
    contexts: Vec<ContextRow>,
}

fn context_rows(keymap: &Keymap) -> Vec<ContextRow> {
    keymap
        .contexts()
        .map(|context| ContextRow {
            context: context.name().to_owned(),
            bindings: keymap
                .bindings_in(context)
                .expect("context came from the keymap")
                .iter()
                .map(|(seq, action)| BindingRow {
                    sequence: seq.iter().map(ToString::to_string).collect(),
                    action: action.id().to_owned(),
                })
                .collect(),
        })
        .collect()
}

/// One snapshot holding all three presets' effective keymaps, per
/// binding context, sequences in canonical display form, sorted
/// (`BTreeMap` order). The `standard` entry doubles as "the resolved
/// default keymap" (§10: default preset + empty overrides ≡ the
/// preset map).
#[test]
fn resolved_preset_keymaps_are_documented() {
    let rows: Vec<PresetRow> = [Preset::Standard, Preset::Emacs, Preset::Vi]
        .into_iter()
        .map(|preset| PresetRow {
            preset: preset.name().to_owned(),
            contexts: context_rows(&Keymap::from_preset(preset)),
        })
        .collect();
    assert_yaml_snapshot!("resolved_preset_keymaps", rows);
}

// -- r4 chunk 4, D15: the F4 investigation's seam records ---------------

use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::actions::ContextId;

/// The projected-hint enumeration (E2): for every preset × every
/// chrome-tooltip action × the board chain `[Board, ViNormal,
/// Global]`, record which `(preset, action)` pairs project NO binding
/// — the honest empty (D19: the hints are keymap projections; an
/// unbound action shows no fake chord). Expected empty set today:
/// exactly `(every preset, BoardSelect)` — the mouse-only board
/// segment. A new binding flips this test deliberately (the plan's
/// pin); the H-b "by-design emptiness" verdict's evidence of record.
///
/// The H-a mask evidence rides along: the presets' mask sets are
/// exactly `{Menu, Palette}` — `ViNormal` never masks, so the mount
/// chain hides nothing at first frame.
#[test]
fn projected_hint_bindings_per_preset() {
    const CHROME_ACTIONS: [ActionId; 7] = [
        ActionId::AppMenu,
        ActionId::NavFocusBoard,
        ActionId::BoardSelect,
        ActionId::TaskEdit,
        ActionId::StackWider,
        ActionId::StackCreate,
        ActionId::TaskCreate,
    ];
    let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
    let mut empty: Vec<(&str, &str)> = Vec::new();
    for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
        let keymap = Keymap::from_preset(preset);
        // H-a: the board chain is unmasked on every shipped preset.
        assert_eq!(
            keymap.visible_chain(&chain),
            chain,
            "{}'s masks never cut the mount chain",
            preset.name()
        );
        // The masks are exactly {Menu, Palette}: a walk into either
        // stops there.
        assert_eq!(keymap.visible_chain(&[ContextId::Menu]), [ContextId::Menu]);
        assert_eq!(
            keymap.visible_chain(&[ContextId::Palette]),
            [ContextId::Palette]
        );
        for action in CHROME_ACTIONS {
            if keymap.first_binding_display(action, &chain).is_none() {
                empty.push((preset.name(), action.id()));
            }
        }
    }
    assert_eq!(
        empty,
        [
            ("standard", "board.select"),
            ("emacs", "board.select"),
            ("vi", "board.select"),
        ],
        "the only unbound projected action is board.select, on every preset"
    );
}

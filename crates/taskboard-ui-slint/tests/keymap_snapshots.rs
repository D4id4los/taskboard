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
}

/// The action registry dump: one row per action, in registry order —
/// id, label, menu-availability contexts, category, and the honest
/// `enabled` bit (§5.8: reserved actions read as disabled).
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
        })
        .collect();
    assert_yaml_snapshot!("registry", rows);
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

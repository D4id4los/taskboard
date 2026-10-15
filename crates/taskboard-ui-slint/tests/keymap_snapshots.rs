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

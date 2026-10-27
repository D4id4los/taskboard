// SPDX-License-Identifier: MIT OR Apache-2.0

//! The board presets table (r3 D5): the named starting layouts the
//! empty-board page can create in one click. Data-only — the stack
//! titles are *domain payloads* (they become `CreateStack.title` on
//! dispatch), not presentation strings, so tests assert them as
//! command payloads. Dispatch lives in the adapter
//! (`apply_board_preset`); the extensibility point is this table plus
//! the preset-parameterised dispatch — exactly one preset ships
//! today, the callback is deliberately not index-parameterised
//! (recorded decision).

/// One named starting layout: an ordered list of stack titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardPreset {
    /// The stable preset name (e.g. `"default-kanban"`).
    pub name: &'static str,
    /// The stacks in creation order (order 0..n on dispatch).
    pub stacks: &'static [&'static str],
}

/// The default Kanban starting layout (the empty-board CTA, R3-F41).
pub const DEFAULT_KANBAN: BoardPreset = BoardPreset {
    name: "default-kanban",
    stacks: &["Backlog", "To Do", "In Progress", "Done"],
};

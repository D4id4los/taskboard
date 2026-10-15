// SPDX-License-Identifier: AGPL-3.0-only

//! The keymap-as-data model (design §5.3): a preset table ⊕ user
//! overrides, validated into an effective [`Keymap`]. Everything here
//! is pure — parsing, merging, and validation never touch slint types
//! or IO (the §5.5 extraction seam); config is read app-side and
//! handed over as typed values.

pub mod stroke;

pub use stroke::{Stroke, StrokeParseError};

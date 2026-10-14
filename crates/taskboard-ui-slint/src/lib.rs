// SPDX-License-Identifier: AGPL-3.0-only
//! Slint presentation layer for `taskboard`.
//!
//! The UI is a "dumb presentation client": it reads state via lock-free
//! `arc_swap::ArcSwap<AppState>` loads, maps fields onto widgets, and emits
//! non-blocking [`tokio::sync::mpsc`] commands on user interaction.
//! Business logic and persistence are forbidden here.
//!
//! NOTE: `#![forbid(unsafe_code)]` is intentionally *not* set in this crate:
//! Slint's generated rendering code requires `unsafe`. This is the only
//! workspace crate with that exception (see `AGENTS.md` and the ADR on
//! toolchain & quality gates).

slint::include_modules!();

pub mod fonts;

#[cfg(test)]
mod test_platform;
#[cfg(test)]
mod token_tests;

#[cfg(test)]
mod font_smoke_tests {
    use slint::ComponentHandle as _;

    use crate::fonts::register_bundled_fonts;
    use crate::test_platform::ensure_null_platform;

    /// D11/D14 smoke: with the null platform registered, the bundled
    /// fonts register, a window component instantiates (its font
    /// properties resolve against the registered families' names), and
    /// the instance drops cleanly — no panic anywhere in the cycle.
    #[test]
    fn registration_and_instantiation_smoke() {
        ensure_null_platform();
        register_bundled_fonts(); // idempotent; a second call is a no-op
        register_bundled_fonts();

        let window = crate::AppWindow::new().expect("window instantiates on the null platform");
        // The theme's Typography globals resolve their family names;
        // reading them exercises the generated global accessors.
        let _ = window.global::<crate::Typography>().get_family_sans();
        let _ = window.global::<crate::Typography>().get_family_mono();
        drop(window);
    }
}

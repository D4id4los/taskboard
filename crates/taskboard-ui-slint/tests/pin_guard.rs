// SPDX-License-Identifier: AGPL-3.0-only

//! T1 — the Slint pin guard (design §8.1/Q12): the
//! `i-slint-backend-testing` dev-pin is exact and equals the `slint`
//! pin in the root manifest, so the two move together on every bump.
//! A structural manifest assertion, not a text assertion on program
//! output.

/// Pulls the version requirement of `name` out of the root manifest's
/// `[workspace.dependencies]`: `slint = "x.y.z"` or the dev-pin's
/// `i-slint-backend-testing = { version = "=x.y.z", … }` inline-table
/// form. `None` when the dependency vanished.
fn version_requirement(manifest: &str, name: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let line = line.trim_start();
        let rest = line.strip_prefix(name)?;
        let rest = rest.strip_prefix(' ')?; // the ` = ` separator
        let rest = rest.strip_prefix('=')?;
        let rest = rest.trim_start();
        if let Some(quoted) = rest.strip_prefix('"') {
            // `slint = "1.18.1"`
            let end = quoted.find('"')?;
            Some(quoted[..end].to_owned())
        } else if let Some(after) = rest.strip_prefix("{ version = \"") {
            // `i-slint-backend-testing = { version = "=1.18.1", … }`
            let end = after.find('"')?;
            Some(after[..end].to_owned())
        } else {
            None
        }
    })
}

#[test]
fn the_testing_backend_pin_is_exact_and_equals_the_slint_pin() {
    let manifest = include_str!("../../../Cargo.toml");
    let slint = version_requirement(manifest, "slint").expect("the slint pin exists");
    let testing = version_requirement(manifest, "i-slint-backend-testing")
        .expect("the testing-backend dev-pin exists");
    // The exact pin shape (§8.1/Q8): internal, non-semver crate.
    let exact = testing.strip_prefix('=').unwrap_or_else(|| {
        panic!("the testing-backend pin must be exact (`=` prefix), is {testing:?}")
    });
    assert_eq!(
        exact, slint,
        "the exact-pinned testing backend must move with the slint pin"
    );
}

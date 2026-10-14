// SPDX-License-Identifier: AGPL-3.0-only
//! Slint compiler wiring: the `.slint` sources under `ui/` are compiled
//! at build time; `slint::include_modules!()` picks the generated code
//! up. Default compiler config — embedded font registration happens at
//! runtime (chunk decision D11), not via the compiler's embed-resources.

fn main() {
    slint_build::compile("ui/app-window.slint").expect("slint compilation failed");
}

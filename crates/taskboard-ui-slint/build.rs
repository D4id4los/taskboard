// SPDX-License-Identifier: AGPL-3.0-only

//! Slint compiler wiring: the `.slint` sources under `ui/` are compiled
//! at build time; `slint::include_modules!()` picks the generated code
//! up. Default compiler config — embedded font registration happens at
//! runtime (chunk decision D11), not via the compiler's embed-resources.

fn main() {
    // Element debug info (element ids/type names/counts) is required
    // by the automation harness: the testing backend's ElementQuery
    // traverses it (upstream: "set SLINT_EMIT_DEBUG_INFO=1 at
    // application build time"), and the MCP lane's element-tree
    // tools read it. The cost is a few names per element.
    slint_build::compile_with_config(
        "ui/app-window.slint",
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("slint compilation failed");

    // The virtualisation probes' host components (#373): a separate
    // generated module so the test tree can instantiate them — the
    // `slint!` macro cannot enable the element debug info the
    // backend's element queries require.
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("build script runs via cargo"),
    );
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let probe_source = manifest.join("tests/virtualisation_probe.slint");
    slint_build::compile_with_output_path(
        &probe_source,
        out.join("virtualisation_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("virtualisation probe compilation failed");
    println!("cargo:rerun-if-changed={}", probe_source.display());

    // The focus-seam probe (round-3 L1): the real TextField's seam
    // against sibling focus containers, same include shape.
    let seam_source = manifest.join("tests/focus_seam_probe.slint");
    slint_build::compile_with_output_path(
        &seam_source,
        out.join("focus_seam_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("focus seam probe compilation failed");
    println!("cargo:rerun-if-changed={}", seam_source.display());
}

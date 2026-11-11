// SPDX-License-Identifier: AGPL-3.0-only

//! Slint compiler wiring: the `.slint` sources under `ui/` are compiled
//! at build time; `slint::include_modules!()` picks the generated code
//! up. Default compiler config — embedded font registration happens at
//! runtime (chunk decision D11), not via the compiler's embed-resources.

use std::path::{Path, PathBuf};

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
    // backend's element queries require. Every probe shares the
    // include shape: tests/<name>.slint → OUT_DIR/<name>.rs, with the
    // rerun-if-changed riding the source (the icon probe additionally
    // reruns on the vendored assets dir, tracked below).
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("build script runs via cargo"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let probes = [
        "virtualisation_probe",
        "focus_coverage_probe",
        "focus_seam_probe",
        "slider_grab_probe",
        "walk_probe",
        "icon_probe",
        "tooltip_layout_probe",
        "dropdown_probe",
        "board_bar_probe",
        "editor_viewport_probe",
        "task_card_title_probe",
    ];
    for name in probes {
        compile_probe(&manifest.join(format!("tests/{name}.slint")), &out);
    }
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("ui/assets/icons").display()
    );
}

/// Compiles one probe component with element debug info (the
/// automation harness's traversal requirement, as above) into
/// `OUT_DIR/<file-stem>.rs`, and pins the rerun on its source.
fn compile_probe(source: &Path, out: &Path) {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("probe sources carry utf-8 file stems");
    slint_build::compile_with_output_path(
        source,
        out.join(format!("{stem}.rs")),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .unwrap_or_else(|err| panic!("{stem} probe compilation failed: {err}"));
    println!("cargo:rerun-if-changed={}", source.display());
}

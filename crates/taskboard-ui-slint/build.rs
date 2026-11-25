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
    // rerun-if-changed riding the source. The registry is DERIVED
    // (#1362): every `tests/*.slint` file is a probe — a new probe
    // file needs no build.rs edit. The derivation is name-sorted so
    // the compile order is deterministic (the goldens' discovery
    // order is stable across runs).
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("build script runs via cargo"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let tests_dir = manifest.join("tests");
    let mut probes: Vec<String> = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|err| panic!("tests dir readable ({}): {err}", tests_dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_str()? != "slint" {
                return None;
            }
            path.file_stem()?.to_str().map(str::to_owned)
        })
        .collect();
    probes.sort();
    for name in &probes {
        compile_probe(&manifest.join(format!("tests/{name}.slint")), &out);
    }
    // The rerun-if-changed on the tests dir itself: the derivation
    // reads it, so a new/removed probe file must rerun this script.
    println!("cargo:rerun-if-changed={}", tests_dir.display());
    // The vendored icon assets: the icon probe (and any other icon
    // consumer) reruns when they change.
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

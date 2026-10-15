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
}

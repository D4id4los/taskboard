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

    // The slider-grab probe (round-3 D20b): the real Slider inside a
    // pannable Flickable — the gesture-steal matrix's host, same
    // include shape.
    let grab_source = manifest.join("tests/slider_grab_probe.slint");
    slint_build::compile_with_output_path(
        &grab_source,
        out.join("slider_grab_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("slider grab probe compilation failed");
    println!("cargo:rerun-if-changed={}", grab_source.display());

    // The control-walk probe (round-3 D25): the synthetic-Tab
    // traversal and the FocusScope focus seam, same include shape.
    let walk_source = manifest.join("tests/walk_probe.slint");
    slint_build::compile_with_output_path(
        &walk_source,
        out.join("walk_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("walk probe compilation failed");
    println!("cargo:rerun-if-changed={}", walk_source.display());

    // The icon colorize probe (r4 chunk 3): the vendored-SVG +
    // `Image.colorize` runtime evidence, same include shape. The
    // assets dir is a tracked dependency: an asset swap must rebuild
    // every composition that embeds it.
    let icon_source = manifest.join("tests/icon_probe.slint");
    slint_build::compile_with_output_path(
        &icon_source,
        out.join("icon_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("icon probe compilation failed");
    println!("cargo:rerun-if-changed={}", icon_source.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("ui/assets/icons").display()
    );

    // The tooltip layout probe (r4 chunk 4): the real TooltipPanel's
    // four-state content matrix, same include shape.
    let tooltip_source = manifest.join("tests/tooltip_layout_probe.slint");
    slint_build::compile_with_output_path(
        &tooltip_source,
        out.join("tooltip_layout_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("tooltip layout probe compilation failed");
    println!("cargo:rerun-if-changed={}", tooltip_source.display());

    // The dropdown probe (r4 chunk 11, D39): the real Dropdown's
    // open/close transitions, keyboard ladder, dismissal, hover
    // seams, and geometry caps — the tier-2 suite's host, same
    // include shape.
    let dropdown_source = manifest.join("tests/dropdown_probe.slint");
    slint_build::compile_with_output_path(
        &dropdown_source,
        out.join("dropdown_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("dropdown probe compilation failed");
    println!("cargo:rerun-if-changed={}", dropdown_source.display());

    // The board-bar threshold probe (r4 chunk 11, D40): the real
    // BoardBar below (strip) and above (dropdown) the segment budget —
    // the adoption journey's host, same include shape.
    let bar_source = manifest.join("tests/board_bar_probe.slint");
    slint_build::compile_with_output_path(
        &bar_source,
        out.join("board_bar_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("board bar probe compilation failed");
    println!("cargo:rerun-if-changed={}", bar_source.display());

    // The editor-viewport probe (round-5 chunk 2): the box/viewport
    // split's growth gate — the fence-lifting verdict's host, same
    // include shape.
    let viewport_source = manifest.join("tests/editor_viewport_probe.slint");
    slint_build::compile_with_output_path(
        &viewport_source,
        out.join("editor_viewport_probe.rs"),
        slint_build::CompilerConfiguration::new().with_debug_info(true),
    )
    .expect("editor viewport probe compilation failed");
    println!("cargo:rerun-if-changed={}", viewport_source.display());
}

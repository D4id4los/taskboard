// SPDX-License-Identifier: AGPL-3.0-only

//! The IconKind→asset mapping's structural totality test (r4 chunk 3,
//! plan §7.1; the `pin_guard` precedent: a structural manifest
//! assertion over the sources, not a text assertion on program
//! output). Three invariants: every enum row is mapped (totality — a
//! future kind without an arm falls to the loud warn fallback, and
//! this test catches it at build time); every asset named in the
//! mapping exists on disk (no missing file → silent warn fallback);
//! every vendored SVG is referenced (no dead vendored files).

const ICON_SOURCE: &str = include_str!("../ui/widgets/icon.slint");
const ICON_BUTTON_SOURCE: &str = include_str!("../ui/widgets/icon-button.slint");

fn enum_rows() -> Vec<String> {
    let start = ICON_SOURCE
        .find("export enum IconKind {")
        .expect("the IconKind enum lives in icon.slint");
    let body = &ICON_SOURCE[start..];
    let end = body.find('}').expect("the enum closes");
    // Strip line comments, then collect the identifier rows.
    body[..end]
        .lines()
        .map(|line| line.split("//").next().expect("line").trim())
        .filter(|line| !line.is_empty() && !line.starts_with("export enum"))
        .map(|line| line.trim_end_matches(',').trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

fn mapping_source() -> &'static str {
    ICON_SOURCE
        .split("source:")
        .nth(1)
        .expect("the Icon component binds a source mapping")
}

fn referenced_assets() -> Vec<String> {
    mapping_source()
        .split("@image-url(")
        .skip(1)
        .map(|chunk| {
            let end = chunk.find(')').expect("each @image-url closes");
            chunk[..end]
                .trim_matches('"')
                .trim_start_matches("../assets/icons/")
                .to_owned()
        })
        .collect()
}

fn vendored_assets() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/assets/icons");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the vendored assets dir is readable")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .map(|path| {
            path.file_name()
                .expect("named")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Every enum row appears in the mapping chain — the vocabulary is
/// mapped total. The enum lives in exactly one file: the vocabulary
/// home (icon.slint), with icon-button.slint re-exporting, not
/// redefining it.
#[test]
fn every_icon_kind_row_is_mapped() {
    let mapping = mapping_source();
    for row in enum_rows() {
        assert!(
            mapping.contains(&format!("IconKind.{row}")),
            "IconKind.{row} has no mapping arm — the mapping is not total"
        );
    }
    assert!(
        !ICON_BUTTON_SOURCE.contains("enum IconKind"),
        "the enum must not be redefined outside its vocabulary home"
    );
}

/// Every asset the mapping names exists on disk — a typo or missing
/// vendored file must not silently fall through to the warn arm.
#[test]
fn every_mapped_asset_exists() {
    let vendored = vendored_assets();
    for asset in referenced_assets() {
        assert!(
            vendored.iter().any(|name| name == &asset),
            "mapped asset {asset} is not vendored under ui/assets/icons/"
        );
    }
}

/// Every vendored SVG is referenced by the mapping — no dead vendored
/// files ride in the binary. The 6 files minted for the r4 growth
/// kinds (dismiss/timer/link/link-off/chevron-down/pencil) are pinned
/// as the exact pending set — they cost nothing until their enum rows
/// land (the same commit removes this list and the exception).
#[test]
fn every_vendored_asset_is_mapped() {
    const PENDING_GROWTH_KINDS: [&str; 6] = [
        "circle-x.svg",
        "timer.svg",
        "link.svg",
        "unlink.svg",
        "chevron-down.svg",
        "pencil.svg",
    ];
    let referenced = referenced_assets();
    for asset in vendored_assets() {
        assert!(
            referenced.iter().any(|name| name == &asset)
                || PENDING_GROWTH_KINDS.contains(&asset.as_str()),
            "vendored asset {asset} is never mapped — a dead file"
        );
    }
}

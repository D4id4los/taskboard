// SPDX-License-Identifier: MIT OR Apache-2.0
//! The substrate tree taxonomy: path constants, pure path builders,
//! and the per-file classifier the transport's directory listings route
//! through (design D-SF2).
//!
//! Paths are forward-slash relative strings **below the substrate
//! root** — never [`std::path::PathBuf`]: the crate has no filesystem,
//! and the transport maps these onto its own medium. Reserved kinds
//! (`ops/*.json`, `snapshot-*.json`) are *structural*: no payload type
//! for them exists in this crate, so no code path can parse them — the
//! "never parse, never delete, never choke" posture enforced by
//! construction. Emission normalises op/snapshot sequence numbers to
//! six digits; decode accepts any all-digit component (documented
//! leniency at [`classify`]).

use crate::ids::DeviceId;

/// The visible root-level folder in the account's file home (Q5).
pub const ROOT_FOLDER_NAME: &str = "taskboard";
/// The per-device subtrees directory.
pub const DEVICES_DIR_NAME: &str = "devices";
/// The create-once folder identity file at the root.
pub const IDENTITY_FILE_NAME: &str = "identity.json";
/// The per-device heartbeat file.
pub const INFO_FILE_NAME: &str = "info.json";
/// The per-device current-state document.
pub const APPDATA_FILE_NAME: &str = "appdata.json";
/// The reserved per-device operations directory (never emitted in v1).
pub const OPS_DIR_NAME: &str = "ops";

/// The tree's format version (`identity.json` payload).
pub const TREE_FORMAT_VERSION: u32 = 1;

#[must_use]
pub fn device_dir_name(id: DeviceId) -> String {
    id.to_string()
}

#[must_use]
pub fn device_info_path(id: DeviceId) -> String {
    format!("{DEVICES_DIR_NAME}/{id}/{INFO_FILE_NAME}")
}

#[must_use]
pub fn device_appdata_path(id: DeviceId) -> String {
    format!("{DEVICES_DIR_NAME}/{id}/{APPDATA_FILE_NAME}")
}

#[must_use]
pub fn device_ops_path(id: DeviceId, seq: u64) -> String {
    format!("{DEVICES_DIR_NAME}/{id}/{OPS_DIR_NAME}/{seq:06}.json")
}

#[must_use]
pub fn device_snapshot_path(id: DeviceId, seq: u64) -> String {
    format!("{DEVICES_DIR_NAME}/{id}/snapshot-{seq:06}.json")
}

/// What a listed relative path is. `Ops`/`Snapshot`/`Temporary` are
/// reserved: relayed opaque, never parsed, never deleted. `Other` is
/// the relay-never-choke arm — anything this version cannot name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFileKind {
    /// The root `identity.json` (create-once, design D-SF2).
    Identity,
    /// A device heartbeat, `devices/<id>/info.json`.
    Info(DeviceId),
    /// A device's current-state document, `devices/<id>/appdata.json`.
    AppData(DeviceId),
    /// A reserved operation-chunk file, `devices/<id>/ops/<digits>.json`.
    Ops { device: DeviceId, seq: u64 },
    /// A reserved snapshot file, `devices/<id>/snapshot-<digits>.json`.
    Snapshot { device: DeviceId, seq: u64 },
    /// A write-in-progress leak (the backbone §7.1 temp discipline):
    /// the final component contains `.tmp-`. Never parsed.
    Temporary,
    /// Anything else — unknown file names, unparsable device
    /// directories. Relayed opaque, never an error.
    Other,
}

/// Classifies a forward-slash relative path below the substrate root.
/// Total over any input string (never panics); strict about file names
/// and about the device component being a parseable [`DeviceId`].
#[must_use]
pub fn classify(relative_path: &str) -> DeviceFileKind {
    let components: Vec<&str> = relative_path.split('/').collect();
    if components.last().is_some_and(|last| last.contains(".tmp-")) {
        return DeviceFileKind::Temporary;
    }
    match components.as_slice() {
        [IDENTITY_FILE_NAME] => DeviceFileKind::Identity,
        [DEVICES_DIR_NAME, device, INFO_FILE_NAME] => {
            classify_device_file(device, DeviceFileKind::Info)
        }
        [DEVICES_DIR_NAME, device, APPDATA_FILE_NAME] => {
            classify_device_file(device, DeviceFileKind::AppData)
        }
        [DEVICES_DIR_NAME, device, OPS_DIR_NAME, ops_file] => {
            classify_reserved_file(device, ops_file, "", |d, seq| DeviceFileKind::Ops {
                device: d,
                seq,
            })
        }
        [DEVICES_DIR_NAME, device, snapshot_file] if snapshot_file.starts_with("snapshot-") => {
            classify_reserved_file(device, snapshot_file, "snapshot-", |d, seq| {
                DeviceFileKind::Snapshot { device: d, seq }
            })
        }
        _ => DeviceFileKind::Other,
    }
}

fn classify_device_file(
    device: &str,
    build: impl Fn(DeviceId) -> DeviceFileKind,
) -> DeviceFileKind {
    DeviceId::parse(device).map_or(DeviceFileKind::Other, build)
}

fn classify_reserved_file(
    device: &str,
    file: &str,
    prefix: &str,
    build: impl Fn(DeviceId, u64) -> DeviceFileKind,
) -> DeviceFileKind {
    let Some(device_id) = DeviceId::parse(device) else {
        return DeviceFileKind::Other;
    };
    let Some(stem) = file
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(".json"))
    else {
        return DeviceFileKind::Other;
    };
    // Leniency (documented at the type): any all-digit stem parses;
    // emission is the six-digit normal form, decode does not demand it.
    if !stem.is_empty()
        && stem.bytes().all(|b| b.is_ascii_digit())
        && let Ok(seq) = stem.parse::<u64>()
    {
        return build(device_id, seq);
    }
    DeviceFileKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::DeviceId;

    fn device() -> DeviceId {
        DeviceId::parse("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a").expect("parses")
    }

    #[test]
    fn classify_table_over_the_full_taxonomy() {
        let id = device();
        assert_eq!(classify(IDENTITY_FILE_NAME), DeviceFileKind::Identity);
        assert_eq!(classify(&device_info_path(id)), DeviceFileKind::Info(id));
        assert_eq!(
            classify(&device_appdata_path(id)),
            DeviceFileKind::AppData(id)
        );
        assert_eq!(
            classify(&device_ops_path(id, 42)),
            DeviceFileKind::Ops {
                device: id,
                seq: 42
            }
        );
        assert_eq!(
            classify("devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a/ops/000042.json"),
            DeviceFileKind::Ops {
                device: id,
                seq: 42
            }
        );
        assert_eq!(
            classify(&device_snapshot_path(id, 128)),
            DeviceFileKind::Snapshot {
                device: id,
                seq: 128
            }
        );
    }

    #[test]
    fn classify_temp_files_by_the_dot_tmp_marker() {
        assert_eq!(
            classify("devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a/info.json.tmp-leakabc"),
            DeviceFileKind::Temporary
        );
        assert_eq!(
            classify("devices/x/appdata.json.tmp-1234"),
            DeviceFileKind::Temporary
        );
    }

    #[test]
    fn classify_unknown_names_and_bad_device_dirs_are_other() {
        assert_eq!(classify("README.md"), DeviceFileKind::Other);
        assert_eq!(
            classify("devices/not-a-uuid/info.json"),
            DeviceFileKind::Other
        );
        assert_eq!(
            classify("devices/not-a-uuid/ops/000001.json"),
            DeviceFileKind::Other
        );
        assert_eq!(classify("other/identity.json"), DeviceFileKind::Other);
        assert_eq!(classify("devices/x/unknown.json"), DeviceFileKind::Other);
        assert_eq!(
            classify("devices/x/ops/notdigits.json"),
            DeviceFileKind::Other
        );
        assert_eq!(classify("devices/x/snapshot-.json"), DeviceFileKind::Other);
        assert_eq!(classify(""), DeviceFileKind::Other);
    }

    #[test]
    fn path_builders_compose_with_the_classifier() {
        let id = DeviceId::mint();
        let expected = [
            (device_info_path(id), DeviceFileKind::Info(id)),
            (device_appdata_path(id), DeviceFileKind::AppData(id)),
            (
                device_ops_path(id, 7),
                DeviceFileKind::Ops { device: id, seq: 7 },
            ),
            (
                device_snapshot_path(id, 9),
                DeviceFileKind::Snapshot { device: id, seq: 9 },
            ),
        ];
        for (path, kind) in expected {
            assert_eq!(classify(&path), kind);
        }
    }

    #[test]
    fn classify_never_panics_over_arbitrary_strings() {
        proptest::proptest!(|(path in ".*")| {
            let _ = classify(&path);
        });
    }
}

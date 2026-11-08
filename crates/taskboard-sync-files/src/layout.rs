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

use uuid::Uuid;

use crate::ids::DeviceId;
use taskboard_domain::ids::BoardId;

/// The visible root-level folder in the account's file home (Q5).
pub const ROOT_FOLDER_NAME: &str = "taskboard";
/// The per-board lifecycle subtrees directory (D-BF6).
pub const BOARDS_DIR_NAME: &str = "boards";
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

/// The per-board identity path, `boards/<board>/identity.json`.
#[must_use]
pub fn board_identity_path(board: BoardId) -> String {
    format!("{BOARDS_DIR_NAME}/{}/{IDENTITY_FILE_NAME}", board.as_uuid())
}

/// The per-(board, device) heartbeat path, `boards/<board>/devices/<device>/info.json`.
#[must_use]
pub fn board_device_info_path(board: BoardId, device: DeviceId) -> String {
    format!(
        "{BOARDS_DIR_NAME}/{}/{DEVICES_DIR_NAME}/{device}/{INFO_FILE_NAME}",
        board.as_uuid()
    )
}

/// The per-(board, device) immutable chunk path,
/// `boards/<board>/devices/<device>/ops/<seq>.json` (six-digit normal form).
#[must_use]
pub fn board_ops_path(board: BoardId, device: DeviceId, seq: u64) -> String {
    format!(
        "{BOARDS_DIR_NAME}/{}/{DEVICES_DIR_NAME}/{device}/{OPS_DIR_NAME}/{seq:06}.json",
        board.as_uuid()
    )
}

/// The per-(board, device) snapshot path,
/// `boards/<board>/devices/<device>/snapshot-<seq>.json` (six-digit normal form).
#[must_use]
pub fn board_snapshot_path(board: BoardId, device: DeviceId, seq: u64) -> String {
    format!(
        "{BOARDS_DIR_NAME}/{}/{DEVICES_DIR_NAME}/{device}/snapshot-{seq:06}.json",
        board.as_uuid()
    )
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
    /// A board subtree's create-once identity, `boards/<board>/identity.json`.
    BoardIdentity { board: BoardId },
    /// A board-lane heartbeat, `boards/<board>/devices/<device>/info.json`
    /// — the per-board file that carries the GC watermark (`last_seq`).
    BoardInfo { board: BoardId, device: DeviceId },
    /// A board's immutable operation chunk,
    /// `boards/<board>/devices/<device>/ops/<digits>.json`.
    BoardOps {
        board: BoardId,
        device: DeviceId,
        seq: u64,
    },
    /// A board's snapshot, `boards/<board>/devices/<device>/snapshot-<digits>.json`.
    BoardSnapshot {
        board: BoardId,
        device: DeviceId,
        seq: u64,
    },
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
        // The board lane routes purely by the `boards/` prefix (D-BF6):
        // a board file can never classify as a root file or vice versa —
        // the arms are structurally disjoint.
        [BOARDS_DIR_NAME, board, IDENTITY_FILE_NAME] => {
            classify_board_file(board, |b| DeviceFileKind::BoardIdentity { board: b })
        }
        [
            BOARDS_DIR_NAME,
            board,
            DEVICES_DIR_NAME,
            device,
            INFO_FILE_NAME,
        ] => classify_board_device_file(board, device, |b, d| DeviceFileKind::BoardInfo {
            board: b,
            device: d,
        }),
        [
            BOARDS_DIR_NAME,
            board,
            DEVICES_DIR_NAME,
            device,
            OPS_DIR_NAME,
            ops_file,
        ] => classify_board_reserved_file(board, device, ops_file, "", |b, d, seq| {
            DeviceFileKind::BoardOps {
                board: b,
                device: d,
                seq,
            }
        }),
        [
            BOARDS_DIR_NAME,
            board,
            DEVICES_DIR_NAME,
            device,
            snapshot_file,
        ] if snapshot_file.starts_with("snapshot-") => {
            classify_board_reserved_file(board, device, snapshot_file, "snapshot-", |b, d, seq| {
                DeviceFileKind::BoardSnapshot {
                    board: b,
                    device: d,
                    seq,
                }
            })
        }
        _ => DeviceFileKind::Other,
    }
}

/// The board component parses as the domain's [`BoardId`] (strict UUID);
/// an unparsable board directory is `Other` — relayed opaque, never an
/// error (the root lane's device-component rule, one level down).
fn classify_board_file(board: &str, build: impl Fn(BoardId) -> DeviceFileKind) -> DeviceFileKind {
    Uuid::parse_str(board).map_or(DeviceFileKind::Other, |id| build(BoardId::from_uuid(id)))
}

fn classify_board_device_file(
    board: &str,
    device: &str,
    build: impl Fn(BoardId, DeviceId) -> DeviceFileKind,
) -> DeviceFileKind {
    let Some(board_id) = Uuid::parse_str(board).ok().map(BoardId::from_uuid) else {
        return DeviceFileKind::Other;
    };
    DeviceId::parse(device).map_or(DeviceFileKind::Other, |d| build(board_id, d))
}

fn classify_board_reserved_file(
    board: &str,
    device: &str,
    file: &str,
    prefix: &str,
    build: impl Fn(BoardId, DeviceId, u64) -> DeviceFileKind,
) -> DeviceFileKind {
    let Some(board_id) = Uuid::parse_str(board).ok().map(BoardId::from_uuid) else {
        return DeviceFileKind::Other;
    };
    let Some(device_id) = DeviceId::parse(device) else {
        return DeviceFileKind::Other;
    };
    let Some(seq) = reserved_seq_stem(file, prefix) else {
        return DeviceFileKind::Other;
    };
    build(board_id, device_id, seq)
}

/// The all-digit stem of a reserved file name (`<prefix><digits>.json`),
/// or `None`. Leniency (documented at the type): any all-digit stem
/// parses; emission is the six-digit normal form, decode does not
/// demand it.
fn reserved_seq_stem(file: &str, prefix: &str) -> Option<u64> {
    let stem = file.strip_prefix(prefix)?.strip_suffix(".json")?;
    if stem.is_empty() || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    stem.parse().ok()
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
    let Some(seq) = reserved_seq_stem(file, prefix) else {
        return DeviceFileKind::Other;
    };
    build(device_id, seq)
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

    fn board() -> BoardId {
        BoardId::from_uuid(Uuid::parse_str("0193b400-0000-7c89-ab01-2345abcdef01").expect("uuid"))
    }

    #[test]
    fn board_arm_classification_table() {
        let board = board();
        let device = device();
        assert_eq!(
            classify(&board_identity_path(board)),
            DeviceFileKind::BoardIdentity { board }
        );
        assert_eq!(
            classify(&board_device_info_path(board, device)),
            DeviceFileKind::BoardInfo { board, device }
        );
        assert_eq!(
            classify(&board_ops_path(board, device, 42)),
            DeviceFileKind::BoardOps {
                board,
                device,
                seq: 42
            }
        );
        assert_eq!(
            classify(&board_snapshot_path(board, device, 128)),
            DeviceFileKind::BoardSnapshot {
                board,
                device,
                seq: 128
            }
        );
    }

    #[test]
    fn board_path_builders_compose_with_the_classifier() {
        let board = board();
        let device = DeviceId::mint();
        for (path, kind) in [
            (
                board_identity_path(board),
                DeviceFileKind::BoardIdentity { board },
            ),
            (
                board_device_info_path(board, device),
                DeviceFileKind::BoardInfo { board, device },
            ),
            (
                board_ops_path(board, device, 7),
                DeviceFileKind::BoardOps {
                    board,
                    device,
                    seq: 7,
                },
            ),
            (
                board_snapshot_path(board, device, 9),
                DeviceFileKind::BoardSnapshot {
                    board,
                    device,
                    seq: 9,
                },
            ),
        ] {
            assert_eq!(classify(&path), kind);
        }
    }

    #[test]
    fn board_misfits_are_other() {
        assert_eq!(
            classify("boards/not-a-uuid/identity.json"),
            DeviceFileKind::Other
        );
        assert_eq!(
            classify("boards/not-a-uuid/devices/x/info.json"),
            DeviceFileKind::Other
        );
        assert_eq!(
            classify("boards/not-a-uuid/devices/x/ops/000001.json"),
            DeviceFileKind::Other
        );
        assert_eq!(
            classify("boards/not-a-uuid/devices/x/snapshot-000001.json"),
            DeviceFileKind::Other
        );
        let board = board();
        let bad_device = format!("boards/{}/devices/not-a-uuid/info.json", board.as_uuid());
        assert_eq!(classify(&bad_device), DeviceFileKind::Other);
        let unknown_file = format!("boards/{}/devices/x/unknown.json", board.as_uuid());
        assert_eq!(classify(&unknown_file), DeviceFileKind::Other);
        let bad_ops = format!("boards/{}/devices/x/ops/notdigits.json", board.as_uuid());
        assert_eq!(classify(&bad_ops), DeviceFileKind::Other);
        let bad_snapshot = format!("boards/{}/devices/x/snapshot-.json", board.as_uuid());
        assert_eq!(classify(&bad_snapshot), DeviceFileKind::Other);
        assert_eq!(
            classify("boards/other/identity.json"),
            DeviceFileKind::Other
        );
    }

    #[test]
    fn the_tmp_rule_applies_over_board_paths() {
        let board = board();
        let device = device();
        let ops = board_ops_path(board, device, 1);
        assert_eq!(
            classify(&format!("{ops}.tmp-leakabc")),
            DeviceFileKind::Temporary
        );
        let identity = board_identity_path(board);
        assert_eq!(
            classify(&format!("{identity}.tmp-leakabc")),
            DeviceFileKind::Temporary
        );
    }

    /// The lane-routing rule (plan §6.1): the classifier is the only
    /// router — a board file never classifies as a root-lane file or
    /// vice versa, pinned arm-by-arm over identical suffix shapes.
    #[test]
    fn board_and_root_lanes_route_by_the_prefix_alone() {
        let board = board();
        let device = device();
        let root = [
            (device_info_path(device), DeviceFileKind::Info(device)),
            (
                device_ops_path(device, 3),
                DeviceFileKind::Ops { device, seq: 3 },
            ),
            (
                device_snapshot_path(device, 5),
                DeviceFileKind::Snapshot { device, seq: 5 },
            ),
        ];
        let board_lane = [
            (
                board_device_info_path(board, device),
                DeviceFileKind::BoardInfo { board, device },
            ),
            (
                board_ops_path(board, device, 3),
                DeviceFileKind::BoardOps {
                    board,
                    device,
                    seq: 3,
                },
            ),
            (
                board_snapshot_path(board, device, 5),
                DeviceFileKind::BoardSnapshot {
                    board,
                    device,
                    seq: 5,
                },
            ),
        ];
        for (root_path, root_kind) in &root {
            assert_eq!(classify(root_path), *root_kind);
            assert!(
                !matches!(
                    classify(root_path),
                    DeviceFileKind::BoardIdentity { .. }
                        | DeviceFileKind::BoardInfo { .. }
                        | DeviceFileKind::BoardOps { .. }
                        | DeviceFileKind::BoardSnapshot { .. }
                ),
                "a root path never routes to the board lane"
            );
        }
        for (board_path, board_kind) in &board_lane {
            assert_eq!(classify(board_path), *board_kind);
            assert!(
                !matches!(
                    classify(board_path),
                    DeviceFileKind::Info(_)
                        | DeviceFileKind::AppData(_)
                        | DeviceFileKind::Ops { .. }
                        | DeviceFileKind::Snapshot { .. }
                ),
                "a board path never routes to the root lane"
            );
        }
    }

    /// The board component parses as the domain's `BoardId` — the
    /// classified arm carries the parsed value, not the raw text.
    #[test]
    fn the_board_component_parses_as_a_board_id() {
        let raw = "0193B400-0000-7C89-AB01-2345ABCDEF01";
        let path = format!("boards/{raw}/identity.json");
        let kind = classify(&path);
        let DeviceFileKind::BoardIdentity { board } = kind else {
            panic!("an uppercase-but-valid board uuid classifies");
        };
        assert_eq!(
            board.as_uuid(),
            Uuid::parse_str(raw).expect("uuid"),
            "the uuid parses case-insensitively like the DeviceId rule"
        );
    }
}

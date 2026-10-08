// SPDX-License-Identifier: MIT OR Apache-2.0
//! Nextcloud OCS response envelope types.
//!
//! Deck REST responses wrap their payload in
//! `{"ocs": {"meta": {...}, "data": ...}}`; unknown fields are ignored so a
//! newer server cannot break decoding.

use serde::Deserialize;

/// Top-level OCS envelope around every Deck API payload.
#[derive(Debug, Clone, Deserialize)]
pub struct OcsEnvelope<T> {
    pub ocs: OcsBody<T>,
}

/// Body of the OCS envelope: metadata plus the actual payload.
#[derive(Debug, Clone, Deserialize)]
pub struct OcsBody<T> {
    pub meta: OcsMeta,
    pub data: T,
}

/// OCS request metadata. Only `statuscode` is consumed; everything else
/// (status, message, items-per-page, etc.) varies across server versions.
#[derive(Debug, Clone, Deserialize)]
pub struct OcsMeta {
    pub statuscode: u16,
}

/// A Deck board as returned by the boards endpoints.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Board {
    pub id: u64,
    pub title: String,
    pub color: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/deck")
            .join(name);
        std::fs::read(path).expect("fixture file must exist")
    }

    #[test]
    fn boards_list_fixture_decodes() {
        let env: OcsEnvelope<Vec<Board>> =
            serde_json::from_slice(&fixture("boards_list.json")).unwrap();
        let boards = env.ocs.data;
        assert_eq!(env.ocs.meta.statuscode, 200);
        assert_eq!(boards.len(), 2);
        assert_eq!(boards[0].title, "taskboard-sync");
        assert_eq!(boards[1].id, 7);
    }

    #[test]
    fn board_create_fixture_decodes_with_unknown_fields() {
        let env: OcsEnvelope<Board> =
            serde_json::from_slice(&fixture("board_create_response.json")).unwrap();
        assert_eq!(env.ocs.data.id, 42);
        assert_eq!(env.ocs.data.color, "ff00ff");
    }

    #[test]
    fn malformed_fixture_is_rejected() {
        let res: Result<OcsEnvelope<Board>, _> =
            serde_json::from_slice(&fixture("envelope_error.json"));
        assert!(res.is_err(), "truncated envelope must not decode");
    }
}

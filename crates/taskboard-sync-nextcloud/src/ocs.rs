// SPDX-License-Identifier: MIT OR Apache-2.0
//! Nextcloud OCS response envelope types.
//!
//! Deck REST responses wrap their payload in
//! `{"ocs": {"meta": {...}, "data": ...}}`; unknown fields are ignored so a
//! newer server cannot break decoding. The resource DTOs live in
//! [`crate::model`].

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
    fn boards_list_envelope_decodes() {
        let env: OcsEnvelope<serde_json::Value> =
            serde_json::from_slice(&fixture("boards_list.json")).unwrap();
        assert_eq!(env.ocs.meta.statuscode, 200);
        assert!(env.ocs.data.is_array());
    }

    #[test]
    fn malformed_fixture_is_rejected() {
        let res: Result<OcsEnvelope<serde_json::Value>, _> =
            serde_json::from_slice(&fixture("envelope_error.json"));
        assert!(res.is_err(), "truncated envelope must not decode");
    }
}

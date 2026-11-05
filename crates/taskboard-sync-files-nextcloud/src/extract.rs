// SPDX-License-Identifier: MIT OR Apache-2.0
//! The fixed PROPFIND request bodies and the narrow 207 extractor
//! (decision D-FT5).
//!
//! Request bodies are **public constants** — the parse surface is
//! closed to what we request (`getetag` only; adding a property means
//! adding its parse arm and its test, in one commit, later). Responses
//! are parsed by a tiny hand-rolled scanner, no XML crate (D-SF17):
//! inside a well-formed `multistatus` tree it locates each `response`
//! element by **local name** (prefix-agnostic — recorded leniency: the
//! namespace URI is never verified) and captures the first `href`
//! text plus the optional `getetag` text. Everything else
//! (`propstat`, `status`, anything unrequested) is ignored.
//!
//! Typed outcomes only: [`Ok(PropfindListing)`] or
//! [`PropfindParseError`]. A `multistatus` with zero `response`
//! members is a legal empty listing (first attach), not an error. The
//! extractor never panics — the fuzz targets below are its contract.
//! The one normalisation allowed: one trailing `/` stripped from
//! collection hrefs (Nextcloud serves collection members with
//! trailing slashes); no percent-decoding exists (the cycle matches
//! raw hrefs against the encoded forms it can compute, D-FT2).

use crate::etag::Etag;
use crate::transport::{PropfindListing, PropfindMember};

/// The Depth-1 PROPFIND request body (`getetag` only). One body serves
/// every Depth-1 listing the design contemplates.
pub const PROPFIND_DEPTH1_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
     <d:propfind xmlns:d=\"DAV:\">\n  <d:prop><d:getetag/></d:prop>\n</d:propfind>";

/// Typed parse rejection: the body was not a well-formed `multistatus`
/// document (malformed XML, truncated, missing/non-XML root). Garbage
/// floors here; there is no "partially parsed" outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not a well-formed multistatus body")]
pub struct PropfindParseError;

/// Extracts the listing from a 207 multi-status body.
///
/// # Errors
///
/// [`PropfindParseError`] when the body is not a well-formed
/// `multistatus` document (malformed XML, truncated, non-XML root).
pub fn extract_207(body: &[u8]) -> Result<PropfindListing, PropfindParseError> {
    let text = std::str::from_utf8(body).map_err(|_| PropfindParseError)?;
    let (open_start, open_end) =
        find_element_bounds(text, "multistatus").ok_or(PropfindParseError)?;
    let inner = &text[open_end..open_start];
    let mut members = Vec::new();
    let mut cursor = 0;
    while let Some((response_start, response_end)) =
        find_element_bounds(&inner[cursor..], "response")
    {
        let response = &inner[cursor + response_end..cursor + response_start];
        if let Some(member) = member_of(response) {
            members.push(member);
        }
        cursor += response_start;
    }
    Ok(PropfindListing { members })
}

/// Finds the byte bounds `(close_of_open_tag, open_of_close_tag)` of
/// the first element with the given local name: `<[prefix:]name …>` …
/// `</[prefix:]name>`. `None` when the element never closes (the
/// truncated-body rejection).
fn find_element_bounds(text: &str, local_name: &str) -> Option<(usize, usize)> {
    let (open, self_closing) = find_open_tag(text, local_name)?;
    if self_closing {
        // `<[p:]name/>`: no content, bounds collapse to the tag end.
        return Some((open, open));
    }
    let rest = &text[open..];
    let close = find_close_tag(rest, local_name)?;
    Some((open + close, open))
}

/// Offset of the end of the first `<[prefix:]name …>` open tag whose
/// local name matches.
fn find_open_tag(text: &str, local_name: &str) -> Option<(usize, bool)> {
    let mut search = 0;
    while let Some(rel) = text[search..].find('<') {
        let start = search + rel;
        let end = text[start..].find('>')?;
        let tag = &text[start + 1..start + end];
        if local_name_of(tag) == Some(local_name) && !tag.starts_with('/') {
            let self_closing = tag.ends_with('/');
            return Some((start + end + 1, self_closing));
        }
        search = start + 1;
    }
    None
}

/// Offset of the start of the first `</[prefix:]name>` close tag whose
/// local name matches.
fn find_close_tag(text: &str, local_name: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(rel) = text[search..].find("</") {
        let start = search + rel;
        let end = text[start..].find('>')?;
        let tag = &text[start + 2..start + end];
        if local_name_of(tag) == Some(local_name) {
            return Some(start);
        }
        search = start + 1;
    }
    None
}

/// The local name of a tag's inner text: `<d:href attr="…">` → `href`.
/// `None` for closing markers and junk.
fn local_name_of(tag_inner: &str) -> Option<&str> {
    let name = tag_inner.split_whitespace().next()?;
    let name = name.strip_prefix('/').unwrap_or(name);
    name.rsplit(':').next()
}

/// Builds the member from one `response` element's inner text: the
/// first `href` text (mandatory — a response without one is skipped as
/// a malformed *member*, not a body failure) and the `getetag` text
/// when present (a collection may omit it).
fn member_of(response_inner: &str) -> Option<PropfindMember> {
    let (_, href_open) = find_element_bounds(response_inner, "href")?;
    let close = find_close_tag(&response_inner[href_open..], "href")?;
    let href = response_inner[href_open..href_open + close].trim();
    if href.is_empty() {
        return None;
    }
    // The one allowed normalisation: one trailing `/` off collection
    // hrefs (never more — a doubled slash stays, it is not ours to fix).
    let href = href.strip_suffix('/').unwrap_or(href);
    let etag = element_text(response_inner, "getetag").map(Etag::new);
    Some(PropfindMember {
        href: href.to_owned(),
        etag,
    })
}

/// The trimmed text content of the first `name` element, if present.
fn element_text(scope: &str, local_name: &str) -> Option<String> {
    let (_, open) = find_element_bounds(scope, local_name)?;
    let close = find_close_tag(&scope[open..], local_name)?;
    Some(scope[open..open + close].trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn extract_str(body: &str) -> Result<PropfindListing, PropfindParseError> {
        extract_207(body.as_bytes())
    }

    fn device_dir_href(device: &str) -> String {
        format!("/remote.php/dav/files/alice/taskboard/devices/{device}/")
    }

    /// A realistic two-member 207 (the docker tier's shape: quoted
    /// etags, collection hrefs with trailing slashes, `propstat`
    /// wrappers and `status` lines the extractor must ignore).
    fn sample_207() -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <d:multistatus xmlns:d=\"DAV:\" xmlns:s=\"http://sabredav.org/ns\" xmlns:oc=\"http://owncloud.org/ns\" xmlns:nc=\"http://nextcloud.org/ns\">\n\
             <d:response>\n\
             <d:href>{a}</d:href>\n\
             <d:propstat>\n\
             <d:prop>\n\
             <d:getetag>&quot;68abc&quot;</d:getetag>\n\
             </d:prop>\n\
             <d:status>HTTP/1.1 200 OK</d:status>\n\
             </d:propstat>\n\
             </d:response>\n\
             <d:response>\n\
             <d:href>{b}</d:href>\n\
             <d:propstat>\n\
             <d:prop/>\n\
             <d:status>HTTP/1.1 404 Not Found</d:status>\n\
             </d:propstat>\n\
             </d:response>\n\
             </d:multistatus>",
            a = device_dir_href("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a"),
            b = device_dir_href("0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5b"),
        )
    }

    #[test]
    fn the_depth1_body_is_the_fixed_getetag_only_contract() {
        // A wire constant, pinned byte-exact: the surface we request is
        // exactly one property, in the DAV namespace.
        assert_eq!(
            PROPFIND_DEPTH1_BODY,
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <d:propfind xmlns:d=\"DAV:\">\n  <d:prop><d:getetag/></d:prop>\n</d:propfind>"
        );
    }

    #[test]
    fn a_two_member_listing_extracts_href_and_etag() {
        let listing = extract_str(&sample_207()).expect("parses");
        assert_eq!(listing.members.len(), 2);
        let expected_href =
            "/remote.php/dav/files/alice/taskboard/devices/0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
        // The collection's one trailing slash is stripped, the etag
        // captured verbatim (its XML entity form as served).
        assert_eq!(listing.members[0].href, expected_href);
        assert_eq!(
            listing.members[0].etag.as_ref().map(Etag::as_ref),
            Some("&quot;68abc&quot;")
        );
    }

    #[test]
    fn a_response_without_getetag_yields_an_etagless_member() {
        let listing = extract_str(&sample_207()).expect("parses");
        assert_eq!(listing.members[1].etag, None);
    }

    #[test]
    fn an_unprefixed_dav_tree_parses_the_same() {
        let body = "<multistatus><response><href>/a/</href>\
                    <propstat><prop><getetag>\"x\"</getetag></prop></propstat>\
                    </response></multistatus>";
        let listing = extract_str(body).expect("parses");
        assert_eq!(listing.members.len(), 1);
        assert_eq!(listing.members[0].href, "/a");
    }

    #[test]
    fn an_empty_multistatus_is_a_legal_empty_listing() {
        let listing = extract_str("<d:multistatus xmlns:d=\"DAV:\"/>").expect("parses");
        assert_eq!(listing.members.len(), 0);
        let listing = extract_str("<multistatus></multistatus>").expect("parses");
        assert_eq!(listing.members.len(), 0);
    }

    #[test]
    fn a_response_without_href_is_skipped_not_fatal() {
        let body = "<multistatus>\
                    <response><propstat><prop><getetag>\"x\"</getetag></prop></propstat></response>\
                    <response><href>/real</href></response>\
                    </multistatus>";
        let listing = extract_str(body).expect("parses");
        assert_eq!(listing.members.len(), 1);
        assert_eq!(listing.members[0].href, "/real");
    }

    #[test]
    fn non_multistatus_truncated_and_non_utf8_bodies_are_typed_rejections() {
        for body in [
            "<?xml version=\"1.0\"?><propfind xmlns:d=\"DAV:\"/>",
            "",
            "totally not xml",
            "<d:multistatus><d:response><d:href>", // truncated
            "<multistatus>",                       // never closed
        ] {
            assert_eq!(extract_str(body), Err(PropfindParseError));
        }
        assert_eq!(extract_207(&[0xff, 0xfe, 0x3c]), Err(PropfindParseError));
    }

    // ---- the fuzz targets (D-FT5; testing-strategy §11 trigger) ----

    proptest::proptest! {
        /// Arbitrary bytes are always a typed outcome, never a panic —
        /// the floor on garbage.
        #[test]
        fn extractor_never_panics_over_arbitrary_bytes(body in proptest::collection::vec(0u8..255, 0..512)) {
            let _ = extract_207(&body);
        }

        /// Structured tearing of a valid sample: truncation, splices,
        /// and byte flips stay typed outcomes; and whenever the
        /// extractor *does* parse, every member it reports carries a
        /// non-empty href (structural floor, no ceilings).
        #[test]
        fn extractor_survives_structured_tearing(
            cut in 0usize..64,
            flip_at in 0usize..64,
            flip_byte in 0u8..255,
            splice_len in 0usize..16,
        ) {
            let mut torn = sample_207().into_bytes();
            torn.truncate(cut.min(torn.len()));
            let sample = sample_207().into_bytes();
            let splice_at = cut.min(sample.len());
            torn.extend_from_slice(&sample[splice_at..(splice_at + splice_len).min(sample.len())]);
            if let Some(slot) = torn.get_mut(flip_at) {
                *slot = flip_byte;
            }
            if let Ok(listing) = extract_207(&torn) {
                for member in &listing.members {
                    prop_assert!(!member.href.is_empty());
                }
            }
        }

        /// The GET-body consumer's contract: arbitrary bytes decode to
        /// a typed outcome, never a panic (the codec fuzz hook riding
        /// this chunk's transport boundary).
        #[test]
        fn document_decode_never_panics_over_arbitrary_bytes(body in proptest::collection::vec(0u8..255, 0..512)) {
            let _ = serde_json::from_slice::<taskboard_sync_files::document::UserDocument>(&body);
        }
    }
}

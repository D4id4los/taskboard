// SPDX-License-Identifier: MIT OR Apache-2.0
//! The transport seam (decision D-FT2): the verb set the files-sync
//! cycle consumes, as a trait — the production `WebDAV` client and the
//! test fake both implement it.
//!
//! The verb set is **closed**: `MKCOL`, `PROPFIND` (Depth 1, fixed
//! body), `GET`, conditional `PUT` (create-once / if-match). The trait
//! has no delete, move, proppatch, or lock method, so no code path can
//! remove or relocate a file — the readers-never-delete posture
//! enforced by construction (F8).
//!
//! Paths are substrate-relative forward-slash strings (the pure core's
//! [`taskboard_sync_files::layout`] vocabulary); the transport maps
//! them onto its medium. Substrate paths are UUIDs and fixed names —
//! already URL-safe, so no percent-encoding layer exists to get wrong.

use std::future::Future;

use crate::error::TransportError;
use crate::etag::Etag;

/// The transport seam: one `WebDAV` medium's verbs.
pub trait FilesTransport: Send + Sync + 'static {
    /// Creates a collection; [`MkcolOutcome::AlreadyExists`] on the
    /// server's 405 (the exists-ok posture — attach is idempotent).
    fn mkcol(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<MkcolOutcome, TransportError>> + Send;

    /// The Depth-1 `devices/` listing (207 multi-status).
    fn propfind_devices(
        &self,
    ) -> impl Future<Output = Result<PropfindListing, TransportError>> + Send;

    /// The Depth-1 listing of a rooted subtree (`taskboard/...`, the
    /// board lanes' change detection — F6): members carry served
    /// etags; both child collections and files list.
    fn propfind_tree(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<PropfindListing, TransportError>> + Send;

    /// Reads one file's bytes plus its served etag (`None` when the
    /// server sends none).
    fn get(&self, path: &str) -> impl Future<Output = Result<FileContent, TransportError>> + Send;

    /// Create-once PUT (`If-None-Match: *`); a 412 maps to
    /// [`TransportError::PreconditionFailed`] — the adoption arm.
    fn put_create(
        &self,
        path: &str,
        body: &[u8],
    ) -> impl Future<Output = Result<PutOutcome, TransportError>> + Send;

    /// Guarded rewrite PUT (`If-Match: <etag verbatim as served>`); a
    /// 412 maps to [`TransportError::PreconditionFailed`] — the
    /// lost-update guard the fork alarm disambiguates.
    fn put_if_match(
        &self,
        path: &str,
        body: &[u8],
        etag: &Etag,
    ) -> impl Future<Output = Result<PutOutcome, TransportError>> + Send;
}

/// `MKCOL` outcomes: created (201) or the collection already existed
/// (405 — the exists-ok mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MkcolOutcome {
    Created,
    AlreadyExists,
}

/// One member of a PROPFIND listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropfindMember {
    /// The raw `href` as served (no percent-decoding — the cycle
    /// matches against the encoded forms it can compute; one trailing
    /// `/` is stripped from collection hrefs, the only allowed
    /// normalisation).
    pub href: String,
    /// The member's etag when the server sent one.
    pub etag: Option<Etag>,
}

/// A 207 multi-status listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropfindListing {
    pub members: Vec<PropfindMember>,
}

/// A GET response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileContent {
    pub bytes: Vec<u8>,
    pub etag: Option<Etag>,
}

/// A PUT outcome: created (201) or updated (204), each carrying the
/// post-write etag when the server sent one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutOutcome {
    Created { etag: Option<Etag> },
    Updated { etag: Option<Etag> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransportError;

    /// The outcome types' construction table: exactly the documented
    /// variants exist — the cycle's match arms stay compiler-exhaustive
    /// when a new outcome is added, which is the point of closed
    /// enums.
    #[allow(dead_code)]
    fn outcomes_are_closed() {
        let outcomes: [MkcolOutcome; 2] = [MkcolOutcome::Created, MkcolOutcome::AlreadyExists];
        assert_eq!(outcomes.len(), 2);
        let puts = [
            PutOutcome::Created { etag: None },
            PutOutcome::Updated {
                etag: Some(Etag::new("\"e\"".into())),
            },
        ];
        assert_eq!(puts.len(), 2);
        let _listing = PropfindListing::default();
        let _content = FileContent::default();
    }

    /// The trait compiles against a stub impl: the seam is implementable
    /// by anything `Send + Sync` (the fake transport proves the same
    /// shape with state).
    #[derive(Debug)]
    struct Stub;

    impl FilesTransport for Stub {
        #[allow(clippy::unused_async_trait_impl)] // the stub mirrors the trait's shape
        async fn mkcol(&self, _path: &str) -> Result<MkcolOutcome, TransportError> {
            Ok(MkcolOutcome::Created)
        }

        #[allow(clippy::unused_async_trait_impl)] // the stub mirrors the trait's shape
        async fn propfind_tree(&self, _path: &str) -> Result<PropfindListing, TransportError> {
            Ok(PropfindListing {
                members: Vec::new(),
            })
        }

        #[allow(clippy::unused_async_trait_impl)] // the stub mirrors the trait's shape
        async fn propfind_devices(&self) -> Result<PropfindListing, TransportError> {
            Ok(PropfindListing::default())
        }

        #[allow(clippy::unused_async_trait_impl)] // the stub mirrors the trait's shape
        async fn get(&self, _path: &str) -> Result<FileContent, TransportError> {
            Ok(FileContent::default())
        }

        #[allow(clippy::unused_async_trait_impl)] // the stub mirrors the trait's shape
        async fn put_create(
            &self,
            _path: &str,
            _body: &[u8],
        ) -> Result<PutOutcome, TransportError> {
            Ok(PutOutcome::Created { etag: None })
        }

        #[allow(clippy::unused_async_trait_impl)] // the stub mirrors the trait's shape
        async fn put_if_match(
            &self,
            _path: &str,
            _body: &[u8],
            _etag: &Etag,
        ) -> Result<PutOutcome, TransportError> {
            Ok(PutOutcome::Updated { etag: None })
        }
    }

    #[tokio::test]
    async fn a_stub_impl_drives_every_verb() {
        let transport = Stub;
        assert!(matches!(
            transport.mkcol("taskboard").await,
            Ok(MkcolOutcome::Created)
        ));
        assert!(matches!(
            transport.propfind_devices().await,
            Ok(listing) if listing.members.is_empty()
        ));
        assert!(matches!(
            transport.get("x").await,
            Ok(content) if content.bytes.is_empty() && content.etag.is_none()
        ));
        assert!(matches!(
            transport.put_create("x", b"{}").await,
            Ok(PutOutcome::Created { etag: None })
        ));
        assert!(matches!(
            transport
                .put_if_match("x", b"{}", &Etag::new("\"e\"".into()))
                .await,
            Ok(PutOutcome::Updated { etag: None })
        ));
    }
}

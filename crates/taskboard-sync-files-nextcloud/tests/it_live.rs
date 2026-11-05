// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 3 integration tests against the hosted Nextcloud
//! (`docs/testing_strategy.org` §8): the docker tier's F3–F6 pins
//! re-pointed at the real tier — the hosted instance is
//! provider-managed, so server behaviour can drift under the app at
//! upgrade time; the controlled docker image pins the semantics, this
//! tier re-pins them against reality (the characterization §3
//! caveat). Plus the F8 trashbin-presence pin evidencing the
//! runbook's recovery path against the real tier.
//!
//! Secretless by construction: credentials come from a repo-root
//! `.env` (`TASKBOARD_IT_NEXTCLOUD_*`, app password, never the
//! account password); skip, never fail, without them.
//!
//! Run from the repo root:
//!
//! ```sh
//! cargo nextest run -p taskboard-sync-files-nextcloud --run-ignored only \
//!   -E 'test(it_nextcloud_live_files)'
//! ```
//!
//! No quota/throttling assertions anywhere below (measure, don't
//! assume — a ceiling would assert an upper bound on success); drift
//! surfaces as a failing pin to be recorded in §8's known-server
//! matrix.

mod common;

use common::{
    LiveCfg, files_transport, live_config, retrying_server_errors, run_id, with_deadline,
};
use taskboard_sync_files_nextcloud::error::TransportError;
use taskboard_sync_files_nextcloud::etag::Etag;
use taskboard_sync_files_nextcloud::transport::{FilesTransport, PutOutcome};

/// A unique, never-deleted per-run file path under `devices/` (the
/// runs leave their subtrees behind — readers never delete).
fn run_appdata(id: &str) -> String {
    format!("taskboard/devices/{id}/appdata.json")
}

async fn seed_run_dirs(
    cfg: &LiveCfg,
    id: &str,
) -> taskboard_sync_files_nextcloud::client::WebDavFilesTransport {
    let transport = files_transport(cfg);
    for dir in [
        "taskboard",
        "taskboard/devices",
        &format!("taskboard/devices/{id}"),
    ] {
        let _ = transport.mkcol(dir).await.expect("mkcol ladder");
    }
    transport
}

/// F3: create-once twice ⇒ created then typed 412; the winner's bytes
/// are what the server serves.
#[tokio::test]
#[ignore = "requires the live tier (.env TASKBOARD_IT_NEXTCLOUD_*)"]
async fn it_nextcloud_live_files_create_once() {
    let Some(cfg) = live_config() else {
        return; // skip, never fail: tier not configured
    };
    with_deadline(async move {
        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let path = run_appdata(&id);
        let outcome = retrying_server_errors(|| async {
            transport.put_create(&path, b"{\"writer\":1}").await
        })
        .await
        .expect("first create wins");
        assert!(matches!(outcome, PutOutcome::Created { .. }));
        let err = transport
            .put_create(&path, b"{\"writer\":2}")
            .await
            .expect_err("the second create loses the race");
        assert!(matches!(err, TransportError::PreconditionFailed));
        let content = transport.get(&path).await.expect("the file is served");
        assert_eq!(content.bytes, b"{\"writer\":1}", "the winner's bytes serve");
    })
    .await
    .expect("test must finish within the live deadline");
}

/// F4: the lost-update guard — the served etag echoed verbatim
/// updates; a stale etag and the double-quote munge (the
/// characterization's client bug class) both fail typed 412.
#[tokio::test]
#[ignore = "requires the live tier (.env TASKBOARD_IT_NEXTCLOUD_*)"]
async fn it_nextcloud_live_files_lost_update_guard() {
    let Some(cfg) = live_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let path = run_appdata(&id);
        transport.put_create(&path, b"v1").await.expect("create");
        let content = retrying_server_errors(|| async { transport.get(&path).await })
            .await
            .expect("get");
        let served = content.etag.expect("the tier serves etags");
        let outcome = transport
            .put_if_match(&path, b"v2", &served)
            .await
            .expect("verbatim echo updates");
        assert!(matches!(outcome, PutOutcome::Updated { .. }));
        let err = transport
            .put_if_match(&path, b"v3", &served)
            .await
            .expect_err("stale is rejected");
        assert!(matches!(err, TransportError::PreconditionFailed));
        let munged = Etag::new(format!("\"{served}\""));
        let err = transport
            .put_if_match(&path, b"v4", &munged)
            .await
            .expect_err("a re-quoted etag is not the served token");
        assert!(matches!(err, TransportError::PreconditionFailed));
    })
    .await
    .expect("test must finish within the live deadline");
}

/// F5: etags churn on identical re-upload (never a content identity)
/// and are stable across GETs in between.
#[tokio::test]
#[ignore = "requires the live tier (.env TASKBOARD_IT_NEXTCLOUD_*)"]
async fn it_nextcloud_live_files_etag_churn() {
    let Some(cfg) = live_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let path = run_appdata(&id);
        transport
            .put_create(&path, b"same-bytes")
            .await
            .expect("create");
        let first = retrying_server_errors(|| async { transport.get(&path).await })
            .await
            .expect("get")
            .etag
            .expect("etag");
        let again = transport.get(&path).await.expect("get").etag.expect("etag");
        assert_eq!(first, again);
        transport
            .put_if_match(&path, b"same-bytes", &again)
            .await
            .expect("guarded rewrite");
        let after = transport.get(&path).await.expect("get").etag.expect("etag");
        assert_ne!(first, after, "an identical re-PUT churns the etag");
    })
    .await
    .expect("test must finish within the live deadline");
}

/// F6: the Depth-1 listing over `devices/` carries the run's
/// directory with an etag; a child write churns the collection etag —
/// the dirty-marker fact the cycle's change detection leans on.
#[tokio::test]
#[ignore = "requires the live tier (.env TASKBOARD_IT_NEXTCLOUD_*)"]
async fn it_nextcloud_live_files_tree_listing() {
    let Some(cfg) = live_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let listed_dir =
            |members: &[taskboard_sync_files_nextcloud::transport::PropfindMember], id: &str| {
                members
                    .iter()
                    .find(|member| member.href.ends_with(id))
                    .and_then(|member| member.etag.clone())
                    .expect("the run's device directory is listed with an etag")
            };
        let before = listed_dir(
            &retrying_server_errors(|| async { transport.propfind_devices().await })
                .await
                .expect("listing")
                .members,
            &id,
        );
        transport
            .put_create(&run_appdata(&id), b"child")
            .await
            .expect("child write");
        let after = listed_dir(
            &transport.propfind_devices().await.expect("listing").members,
            &id,
        );
        assert_ne!(before, after, "a child write is visible in the tree etag");
    })
    .await
    .expect("test must finish within the live deadline");
}

/// F8: the trashbin is present and lists a deleted file — the
/// runbook's recovery path re-pinned against the real tier. The
/// DELETE is *test scaffolding only* (a raw `reqwest` call; the
/// transport's verb set stays closed — readers never delete).
#[tokio::test]
#[ignore = "requires the live tier (.env TASKBOARD_IT_NEXTCLOUD_*)"]
async fn it_nextcloud_live_files_trashbin_presence() {
    let Some(cfg) = live_config() else {
        return;
    };
    with_deadline(async move {
        use base64::Engine as _;

        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let name = format!("{id}.json");
        let path = format!("taskboard/devices/{id}/{name}");
        transport
            .put_create(&path, b"recoverable")
            .await
            .expect("create");

        // The scaffolding DELETE against the files collection.
        let files_url = format!(
            "{}/remote.php/dav/files/{}/{}",
            cfg.url.trim_end_matches('/'),
            cfg.user,
            path
        );
        let auth = format!("{}:{}", cfg.user, cfg.token);
        let client = reqwest::Client::new();
        let delete = client
            .delete(&files_url)
            .header("Authorization", format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(&auth)))
            .send()
            .await
            .expect("the scaffolding delete completes");
        assert!(
            delete.status().is_success(),
            "the scaffolding delete must succeed: {}",
            delete.status()
        );

        // The trashbin lists the deleted name (Depth-1 PROPFIND, the
        // characterization F8's probe).
        let trash_url = format!(
            "{}/remote.php/dav/trashbin/{}/trash/",
            cfg.url.trim_end_matches('/'),
            cfg.user
        );
        let body = r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/></d:prop></d:propfind>"#;
        // The scaffolding closure speaks the transport's error type so
        // the tier's server-500 retry applies; reqwest's own error
        // kinds are not retried (only sporadic `Server(5xx)`s are).
        let propfind = || async {
            let response = client
                .request(
                    reqwest::Method::from_bytes(b"PROPFIND").expect("a valid verb"),
                    &trash_url,
                )
                .header(
                    "Authorization",
                    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(&auth)),
                )
                .header("Depth", "1")
                .body(body)
                .send()
                .await
                .map_err(|_| TransportError::Server(0))?;
            let status = response.status().as_u16();
            response
                .error_for_status()
                .map_err(|_| TransportError::Server(status))
        };
        let response = retrying_server_errors(propfind)
            .await
            .expect("the trashbin answers 207");
        let listing = response.text().await.expect("the listing body");
        assert!(
            listing.contains(&name),
            "the deleted name must be listed in the trashbin (F8)"
        );
    })
    .await
    .expect("test must finish within the live deadline");
}

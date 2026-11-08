// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 2 integration tests: the files transport's conditional-PUT
//! semantics against the real (dockerized) Nextcloud (`docs/testing_
//! strategy.org` §8) — the characterization F3–F6 facts re-pinned
//! against the controlled image, plus the two server facts the fork
//! disambiguation and the MKCOL exists-ok mapping lean on.
//!
//! Secretless by construction (the setup script mints the app
//! password); skip, never fail, without the tier env. Run locally:
//!
//! ```sh
//! eval "$(scripts/nextcloud_it_setup.sh up)" &&
//!   cargo nextest run -p taskboard-sync-files-nextcloud --run-ignored only \
//!     -E 'test(it_nextcloud_docker_files)'
//! ```

mod common;

use common::{
    LiveCfg, files_transport, live_docker_config, retrying_server_errors, run_id, with_deadline,
};
use taskboard_sync_files_nextcloud::error::TransportError;
use taskboard_sync_files_nextcloud::etag::Etag;
use taskboard_sync_files_nextcloud::transport::{FilesTransport, MkcolOutcome, PutOutcome};

/// A unique, never-deleted per-run file path under `devices/`.
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

/// F3: create-once twice ⇒ 201 then typed 412; the winner's bytes are
/// what the server serves.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_files_create_once() {
    let Some(cfg) = live_docker_config() else {
        return;
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

/// F4: the lost-update guard — `If-Match` echoing the served etag
/// verbatim updates (204); a stale etag and the *double-quoted munge*
/// (the characterization's client bug class) both fail typed 412.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_files_lost_update_guard() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let path = run_appdata(&id);
        transport.put_create(&path, b"v1").await.expect("create");
        let content = transport.get(&path).await.expect("get");
        let served = content.etag.expect("the tier serves etags");
        // The verbatim echo updates.
        let outcome = transport
            .put_if_match(&path, b"v2", &served)
            .await
            .expect("verbatim echo updates");
        assert!(matches!(outcome, PutOutcome::Updated { .. }));
        // A stale etag (from the first GET) is typed 412.
        let err = transport
            .put_if_match(&path, b"v3", &served)
            .await
            .expect_err("stale is rejected");
        assert!(matches!(err, TransportError::PreconditionFailed));
        // The bug class the characterization found: a double-quoted
        // (re-quoted) echo must die at the wire, not pass.
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
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_files_etag_churn() {
    let Some(cfg) = live_docker_config() else {
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
        let first = transport.get(&path).await.expect("get").etag.expect("etag");
        // Stable across GETs.
        let again = transport.get(&path).await.expect("get").etag.expect("etag");
        assert_eq!(first, again);
        // Identical re-PUT: the etag changes (the churn fact the
        // skip-PUT design leans on — no ceilings, one inequality).
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

/// F6: the Depth-1 listing over `devices/` carries member etags; a
/// child write changes the collection's etag — the dirty-marker fact
/// the cycle's change detection leans on.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_files_tree_listing() {
    let Some(cfg) = live_docker_config() else {
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
            &transport.propfind_devices().await.expect("listing").members,
            &id,
        );
        // A child write churns the parent collection's etag.
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

/// The fork disambiguation's server fact (D-FT6): `If-Match` against a
/// nonexistent path answers 412 — not 404.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_files_if_match_missing_is_412() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = seed_run_dirs(&cfg, &id).await;
        let missing = format!("taskboard/devices/{id}/missing.json");
        let err = transport
            .put_if_match(&missing, b"x", &Etag::new("\"anything\"".into()))
            .await
            .expect_err("a guarded PUT at a missing path is typed");
        assert!(
            matches!(err, TransportError::PreconditionFailed),
            "the disambiguation re-keys on this fact: {err:?}"
        );
    })
    .await
    .expect("test must finish within the live deadline");
}

/// The MKCOL exists-ok mapping's live basis: created (201) vs existing
/// (405 ⇒ `AlreadyExists`); a missing parent stays the typed
/// `Conflict`.
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_files_mkcol_posture() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = files_transport(&cfg);
        let created = transport.mkcol("taskboard").await.expect("root mkcol");
        assert_eq!(created, MkcolOutcome::Created);
        let exists = transport.mkcol("taskboard").await.expect("second mkcol");
        assert_eq!(exists, MkcolOutcome::AlreadyExists);
        let err = transport
            .mkcol(&format!("taskboard/devices/{id}/deep"))
            .await
            .expect_err("parent missing");
        assert!(matches!(err, TransportError::Conflict));
    })
    .await
    .expect("test must finish within the live deadline");
}

/// The board subtree's transport facts (dispatch chunk 5, the plan's
/// §4 legs re-pinned against the real WebDAV): the board MKCOL ladder
/// takes nested creation, a chunk publishes create-once (F3), the
/// board tree listing shows the chunk file with an etag (F6), and the
/// snapshot's guarded rewrite answers 412 on a stale etag (F4).
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_board_subtree_semantics() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(async move {
        let id = run_id();
        let transport = files_transport(&cfg);
        let board = format!("taskboard/boards/{id}-board");
        for dir in [
            "taskboard",
            "taskboard/boards",
            &board,
            &format!("{board}/devices"),
            &format!("{board}/devices/{id}"),
            &format!("{board}/devices/{id}/ops"),
        ] {
            let _ = transport.mkcol(dir).await.expect("board mkcol ladder");
        }
        let chunk = format!("{board}/devices/{id}/ops/000001.json");
        retrying_server_errors(|| async {
            transport
                .put_create(&chunk, b"{\"first_seq\":1,\"last_seq\":1}")
                .await
        })
        .await
        .expect("chunk create-once");
        // F3 at the board subtree: a re-create is the typed 412.
        let again = transport.put_create(&chunk, b"duplicate").await;
        assert!(
            matches!(again, Err(TransportError::PreconditionFailed)),
            "board chunk re-create must be the typed 412: {again:?}"
        );
        // F6: the tree listing names the chunk file with an etag.
        let listing = transport
            .propfind_tree(&format!("{board}/devices/{id}/ops"))
            .await
            .expect("board tree listing");
        assert!(
            listing
                .members
                .iter()
                .any(|member| member.href.ends_with("000001.json")),
            "the chunk file is listed"
        );
        // F4: the snapshot's guarded rewrite answers 412 on a stale etag.
        let snapshot = format!("{board}/devices/{id}/snapshot-000001.json");
        let created = retrying_server_errors(|| async {
            transport.put_create(&snapshot, b"{\"seq\":1}").await
        })
        .await
        .expect("snapshot create-once");
        let PutOutcome::Created { etag: Some(etag) } = created else {
            panic!("the create carries the served etag: {created:?}");
        };
        let stale = Etag::new("\"stale\"".to_owned());
        let guarded = transport
            .put_if_match(&snapshot, b"{\"seq\":2}", &stale)
            .await;
        assert!(matches!(guarded, Err(TransportError::PreconditionFailed)));
        let fresh = transport
            .put_if_match(&snapshot, b"{\"seq\":2}", &etag)
            .await;
        assert!(fresh.is_ok(), "the fresh etag rewrites: {fresh:?}");
    })
    .await
    .expect("test must finish within the live deadline");
}

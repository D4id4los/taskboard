// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared harness for the Nextcloud integration tiers (strategy §8).
//!
//! Tier 2 (dockerized) and Tier 3 (live server) tests are `#[ignore]`-marked
//! and skip themselves when their environment is not configured, so the
//! default suite stays hermetic.
//!
//! When a live tier fails on wire-format drift, consult the known-server
//! matrix in `docs/testing_strategy.org` §8 first (Tier 3 live as of
//! 2026-10-08: Nextcloud 33.0.9 / Deck 1.17.5; Tier 2 docker: Nextcloud
//! 35.0.1 / Deck 1.19.0) — it lists each observed deviation from the Deck
//! API reference.
// Not every tier's test binary uses every helper.
#![allow(dead_code)]

pub(crate) mod hermetic;
pub(crate) mod sync_pair;

use std::time::Duration;

use chrono::{DateTime, Utc};
use taskboard_sync_nextcloud::DeckClient;

/// A clock pinned at one instant: the hermetic suites' injected seam, so
/// every LWW-participating stamp the actor mints is deterministic. Pinned
/// after the fixtures' base timestamp (`hermetic::T`) to preserve the
/// real-clock orderings (ops seen by the cycle, local stamps newer than
/// fixture stamps).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FixedClock(pub DateTime<Utc>);

impl taskboard_domain::Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

/// A clock that advances a fixed step per read: the engine+actor pair's
/// seam where later stamps must compare newer (freshness predicates, the
/// self-clobber ordering) while staying deterministic. Whole-second steps
/// keep the stamps trivially readable next to the fixture stamps.
#[derive(Debug)]
pub(crate) struct SteppingClock {
    next_secs: std::sync::atomic::AtomicI64,
    step_secs: i64,
}

impl SteppingClock {
    pub(crate) fn new(start: DateTime<Utc>, step_secs: i64) -> Self {
        Self {
            next_secs: std::sync::atomic::AtomicI64::new(start.timestamp()),
            step_secs,
        }
    }
}

impl taskboard_domain::Clock for SteppingClock {
    fn now(&self) -> DateTime<Utc> {
        use chrono::TimeZone as _;
        let secs = self
            .next_secs
            .fetch_add(self.step_secs, std::sync::atomic::Ordering::Relaxed);
        chrono::Utc
            .timestamp_opt(secs, 0)
            .single()
            .expect("in-range stamp")
    }
}

/// Reads Tier 2 docker-tier credentials; `None` when any is unset.
pub(crate) fn live_docker_config() -> Option<LiveCfg> {
    read_cfg("TASKBOARD_IT_DOCKER")
}

/// Reads the Deck 1.17.5 gate-tier credentials (the second instance pinned
/// via `DECK_VERSION=v1.17.5` + `TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache`,
/// exported under the `TASKBOARD_IT_DOCKER_DECK1175` prefix); `None` when
/// unset — skip, never fail.
pub(crate) fn live_docker_deck1175_config() -> Option<LiveCfg> {
    read_cfg("TASKBOARD_IT_DOCKER_DECK1175")
}

/// Reads Tier 3 live-server credentials from the environment (after loading
/// the repo-root `.env` when present); `None` when any is unset.
pub(crate) fn live_config() -> Option<LiveCfg> {
    let _ = dotenvy::dotenv();
    read_cfg("TASKBOARD_IT_NEXTCLOUD")
}

#[derive(Debug, Clone)]
pub(crate) struct LiveCfg {
    pub url: String,
    pub user: String,
    pub token: String,
}

fn read_cfg(prefix: &str) -> Option<LiveCfg> {
    let (url, user, token) = (
        std::env::var(format!("{prefix}_URL")).ok()?,
        std::env::var(format!("{prefix}_USER")).ok()?,
        std::env::var(format!("{prefix}_TOKEN")).ok()?,
    );
    if url.is_empty() || user.is_empty() || token.is_empty() {
        return None;
    }
    Some(LiveCfg { url, user, token })
}

/// Unique per-run prefix for test resources so parallel/never-deleting runs
/// cannot collide on the server.
pub(crate) fn run_id() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("taskboard-it-{}", &id[..8])
}

/// Wall-clock cap for every live/docker test body.
pub(crate) const LIVE_DEADLINE: Duration = Duration::from_secs(30);

/// Runs `body` under a hard deadline so a hung server fails the test instead
/// of wedging CI.
pub(crate) async fn with_deadline<T>(
    body: impl std::future::Future<Output = T>,
) -> Result<T, tokio::time::error::Elapsed> {
    tokio::time::timeout(LIVE_DEADLINE, body).await
}

/// Retries `op` while Deck answers one of its sporadic write-path
/// `Server(500)`s (known upstream flakiness, observed across create/reorder/
/// archive/delete; see the known-server matrix in `docs/testing_strategy.org`).
/// The production client deliberately does *not* retry `Server` — a retried
/// create can double-apply — but tier scaffolding operates on per-run
/// resources where a retry is safe, and reads are always safe to retry.
pub(crate) async fn retrying_server_errors<T>(
    mut op: impl AsyncFnMut() -> Result<T, taskboard_sync_nextcloud::DeckError>,
) -> Result<T, taskboard_sync_nextcloud::DeckError> {
    let mut attempts = 0u32;
    loop {
        match op().await {
            Err(taskboard_sync_nextcloud::DeckError::Server(_)) if attempts < 4 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(200 * u64::from(attempts))).await;
            }
            other => return other,
        }
    }
}

/// Builds the client for a resolved tier configuration.
pub(crate) fn deck_client(cfg: &LiveCfg) -> DeckClient {
    DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("tier URL must be valid")
}

/// The live-suite behaviors shared by Tier 2 (dockerized) and Tier 3 (live
/// server): both tiers re-run the *same* operations against increasingly
/// real servers, so the bodies live here exactly once and each tier file
/// keeps only config resolution, skip logic, and its `it_nextcloud_*` test
/// names. Client *behavior* (envelope, headers, errors, retries) is asserted
/// at Tiers 0/1 and deliberately not duplicated here. The hermetic suites'
/// shared harness lives in [`hermetic`].
pub(crate) mod suite {
    use super::run_id;
    use taskboard_sync_nextcloud::{DeckClient, DeckColor, DeckError};

    /// Listing succeeds and decodes well-formed boards.
    pub(crate) async fn lists_boards(client: &DeckClient) {
        let boards = client.boards().await.expect("boards listing must succeed");
        assert!(
            boards.iter().all(|b| !b.title.is_empty()),
            "decoded boards must be well-formed"
        );
    }

    /// A run-id board is created, appears in the listing, and its soft
    /// delete is stamped with a non-zero `deletedAt` in the DELETE response.
    pub(crate) async fn board_lifecycle(client: &DeckClient) {
        let title = run_id();

        let created = client
            .create_board(
                &title,
                &DeckColor::from_hex("00c2e0").expect("valid fixture color"),
            )
            .await
            .expect("board creation must succeed");

        let listed = client.boards().await.expect("boards listing must succeed");
        assert!(
            listed
                .iter()
                .any(|b| b.id == created.id && b.title == title),
            "created board must appear in the listing"
        );

        // Deck's DELETE is a soft delete, and the DELETE response is the
        // authoritative, immediately-consistent record of it (the boards
        // listing can lag behind Deck's board cache).
        let deleted = client
            .delete_board(created.id)
            .await
            .expect("board deletion must succeed");
        assert!(
            !deleted.is_live(),
            "DELETE response must stamp deletedAt (got {deleted:?})"
        );
    }

    /// Full write-surface lifecycle on a run-id board: label, two stacks,
    /// two cards, duedate + label assignment, archive, cross-stack move,
    /// card round-trip update, then a read-back of the whole tree. Soft
    /// delete is asserted only on the authoritative DELETE payload (the
    /// boards-listing lag precedent); the caller tears the board down.
    pub(crate) async fn full_tree_lifecycle(client: &DeckClient) {
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid lifecycle color");

        let board = client
            .create_board(&run, &color)
            .await
            .expect("board creation must succeed");

        let result = tree_body(client, board.id, &run, &color).await;

        // Best-effort teardown regardless of body outcome; a failure here
        // must not mask the body's error.
        let deleted = client.delete_board(board.id).await;
        match (result, deleted) {
            (Ok(()), Ok(d)) => assert!(!d.is_live(), "DELETE payload must stamp deletedAt"),
            (Err(body_err), _) => panic!("full-tree lifecycle failed: {body_err:?}"),
            (Ok(()), Err(teardown_err)) => panic!("board teardown failed: {teardown_err:?}"),
        }
    }

    async fn tree_body(
        client: &DeckClient,
        board_id: u64,
        run: &str,
        color: &taskboard_sync_nextcloud::DeckColor,
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        let label = client
            .create_label(board_id, &format!("{run}-lbl"), color)
            .await?;

        let stack_a = client
            .create_stack(board_id, &format!("{run}-a"), 0)
            .await?;
        let stack_b = client
            .create_stack(board_id, &format!("{run}-b"), 1)
            .await?;

        let duedate = chrono::Utc::now() + chrono::Duration::days(1);
        let card_a = client
            .create_card(
                board_id,
                stack_a.id,
                &taskboard_sync_nextcloud::NewCard {
                    title: format!("{run}-card"),
                    order: Some(0),
                    description: None,
                    duedate: Some(duedate),
                    startdate: None,
                    kind: None,
                },
            )
            .await?;
        let card_b = client
            .create_card(
                board_id,
                stack_a.id,
                &taskboard_sync_nextcloud::NewCard {
                    title: format!("{run}-archived"),
                    order: Some(1),
                    description: None,
                    duedate: None,
                    startdate: None,
                    kind: None,
                },
            )
            .await?;

        client
            .assign_label(board_id, stack_a.id, card_a.id, label.id)
            .await?;
        client.archive_card(board_id, stack_a.id, card_b.id).await?;

        // Cross-stack move (the reorder primitive): card_a to stack_b.
        client
            .reorder_card(board_id, stack_a.id, card_a.id, 0, stack_b.id)
            .await?;

        // Full round-trip update: fetch by id (the wire card's own stackId
        // is authoritative — the move itself can lag Deck's cache).
        let mut fetched = client.card(board_id, stack_a.id, card_a.id).await?;
        fetched.description = format!("{run}-description");
        client.update_card(board_id, &fetched).await?;

        // Read the tree back and assert terminal state: the card must be
        // listed under one of the board's stacks with all mutations.
        let stacks = client
            .stacks(board_id, taskboard_sync_nextcloud::StackFilter::Active)
            .await?;
        let moved = stacks
            .iter()
            .flat_map(|s| &s.cards)
            .find(|c| c.id == card_a.id)
            .expect("card must be listed under one of the board's stacks");
        assert_eq!(moved.description, format!("{run}-description"));
        assert!(
            moved.duedate.is_some(),
            "duedate must survive the round-trip"
        );
        assert!(
            moved.labels.iter().any(|l| l.id == label.id),
            "label must survive the round-trip"
        );

        // `GET /boards/{id}/labels` is not available on all Deck servers
        // (405 on Nextcloud 35); the board read carries the label list.
        let board_after = client.board(board_id).await?;
        assert!(
            board_after.labels.iter().any(|l| l.id == label.id),
            "created label must be listed on the board"
        );
        Ok(())
    }

    /// Real conditional-read cycle: first poll (validators), conditional
    /// poll (either `304` or fresh data — servers that do not emit
    /// validators on these routes must not fail the test), then a mutation,
    /// then a conditional poll with the *stale* validators which must
    /// return fresh data containing the mutation.
    pub(crate) async fn conditional_read_cycle(client: &DeckClient) {
        let first = client
            .fetch_boards(&taskboard_sync_nextcloud::Validators::default())
            .await
            .expect("first conditional fetch must succeed");
        assert!(first.data.is_some(), "first poll must return data");
        assert!(
            first.validators.etag.is_some() || first.validators.last_modified.is_some(),
            "data-bearing responses should carry validators (server-dependent)"
        );

        // Server-dependent: a validator-aware server answers 304 here.
        if let Ok(cached) = client.fetch_boards(&first.validators).await
            && cached.data.is_none()
        {
            assert_eq!(
                cached.validators, first.validators,
                "304 must keep the validators usable for the next poll"
            );
        }

        // Mutate, then poll conditionally with the stale validators.
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid cycle color");
        let created = client
            .create_board(&run, &color)
            .await
            .expect("mutation board creation must succeed");

        let after = client
            .fetch_boards(&first.validators)
            .await
            .expect("post-mutation conditional fetch must succeed");
        let boards = after
            .data
            .unwrap_or_else(|| panic!("stale validators must not yield 304 after a mutation"));
        assert!(
            boards.iter().any(|b| b.id == created.id),
            "post-mutation listing must contain the new board"
        );

        let deleted = client
            .delete_board(created.id)
            .await
            .expect("teardown delete");
        assert!(!deleted.is_live());
    }

    /// Attachment write + content lifecycle on a run-id board (D1 spike
    /// §3.2/§3.3 evidence, pinned against the real tier): upload, duplicate
    /// refusal, byte-identical download, in-place update, card-stamp
    /// advancement with validator invalidation, soft delete (tombstone
    /// stays listed), restore, and content intact after the restore.
    pub(crate) async fn attachment_content_lifecycle(client: &DeckClient) {
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid lifecycle color");

        let board = client
            .create_board(&run, &color)
            .await
            .expect("board creation must succeed");

        let result = attachment_body(client, board.id, &run).await;

        // Best-effort teardown regardless of body outcome; a failure here
        // must not mask the body's error.
        let deleted = client.delete_board(board.id).await;
        match (result, deleted) {
            (Ok(()), Ok(d)) => assert!(!d.is_live(), "DELETE payload must stamp deletedAt"),
            (Err(body_err), _) => panic!("attachment lifecycle failed: {body_err:?}"),
            (Ok(()), Err(teardown_err)) => panic!("board teardown failed: {teardown_err}"),
        }
    }

    /// Creates the run-id stack + card and captures the card's pre-write
    /// stamp plus the validators of that read (re-presented after the
    /// writes, they must yield fresh data — the writes invalidate them).
    #[allow(clippy::type_complexity)]
    async fn setup_target(
        client: &DeckClient,
        board_id: u64,
        run: &str,
    ) -> Result<
        (
            taskboard_sync_nextcloud::Stack,
            taskboard_sync_nextcloud::Card,
            i64,
            taskboard_sync_nextcloud::Validators,
        ),
        taskboard_sync_nextcloud::DeckError,
    > {
        use taskboard_sync_nextcloud::{NewCard, Validators};

        let stack = super::retrying_server_errors(|| async {
            client
                .create_stack(board_id, &format!("{run}-stack"), 0)
                .await
        })
        .await?;
        let card = super::retrying_server_errors(|| async {
            client
                .create_card(
                    board_id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-card"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        startdate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await?;
        let stamp_before = client
            .card(board_id, stack.id, card.id)
            .await?
            .last_modified;
        let validators = client
            .fetch_card(board_id, stack.id, card.id, &Validators::default())
            .await?
            .validators;
        Ok((stack, card, stamp_before, validators))
    }

    async fn attachment_body(
        client: &DeckClient,
        board_id: u64,
        run: &str,
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        use taskboard_sync_nextcloud::AttachmentUpload;

        let upload_name = format!("{run}-sidecar.json");
        let payload_v1 = format!(r#"{{"run":"{run}","seq":1}}"#).into_bytes();
        let payload_v2 = format!(r#"{{"run":"{run}","seq":2}}"#).into_bytes();
        let upload_v1 = || AttachmentUpload {
            file_name: upload_name.clone(),
            mime_type: "application/json".into(),
            content: payload_v1.clone(),
        };
        let upload_v2 = AttachmentUpload {
            content: payload_v2.clone(),
            ..upload_v1()
        };

        let (stack, card, stamp_before, validators) = setup_target(client, board_id, run).await?;

        // Upload: the stored filename comes from the part's client
        // filename, and the decoded size matches the uploaded byte count.
        let created = super::retrying_server_errors(|| async {
            client
                .upload_attachment(board_id, stack.id, card.id, &upload_v1())
                .await
        })
        .await?;
        assert_eq!(
            created
                .data
                .as_ref()
                .expect("create echoes the stored file")
                .file_name,
            upload_name
        );
        assert_eq!(
            created
                .extended_data
                .as_ref()
                .expect("create echoes the size")
                .size,
            i64::try_from(payload_v1.len()).expect("payload size fits i64")
        );
        assert_eq!(created.deleted_at, 0);

        // Create refuses duplicates per (card, filename).
        let duplicate = client
            .upload_attachment(board_id, stack.id, card.id, &upload_v1())
            .await
            .unwrap_err();
        assert!(
            matches!(duplicate, taskboard_sync_nextcloud::DeckError::Conflict),
            "duplicate filename must conflict: {duplicate:?}"
        );

        // Download: byte-identical round-trip (D1's sha256 evidence,
        // asserted as Vec equality).
        let downloaded = client
            .download_attachment(board_id, stack.id, card.id, created.id)
            .await?;
        assert_eq!(downloaded, payload_v1);

        // The content route serves no HTTP validators (D1 §3.2, pinned
        // against the live tier, not just mocked): the typed validator
        // pair re-extracted from the download response must be empty —
        // change detection rides attachment metadata instead.
        let content_validators = client
            .download_attachment_validators(board_id, stack.id, card.id, created.id)
            .await?;
        assert_eq!(
            content_validators,
            taskboard_sync_nextcloud::Validators::default(),
            "the content route must serve no ETag/Last-Modified validators"
        );

        // Stamp observability seam: Deck's stamps are second-granular
        // (roundtrip §7), so a write sequence completing inside the same
        // wall-clock second as the pre-write read advances nothing the
        // conditional reads can observe. Wait (poll the wall clock under
        // the tier deadline) until the current second differs from the
        // pre-write stamp, making the advancement below observable by
        // construction instead of by timing luck.
        {
            let deadline = tokio::time::Instant::now() + super::LIVE_DEADLINE;
            while chrono::Utc::now().timestamp() <= stamp_before {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the wall clock never left the pre-write second ({stamp_before})"
                );
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }

        // In-place update with the same filename: id and createdAt are
        // preserved; second-granularity stamps only guarantee >=.
        let updated = super::retrying_server_errors(|| async {
            client
                .update_attachment_content(board_id, card.id, created.id, &upload_v2)
                .await
        })
        .await?;
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.created_at, created.created_at);
        assert!(
            updated.last_modified >= created.last_modified,
            "update must not move the content stamp backwards"
        );
        let redownloaded = client
            .download_attachment(board_id, stack.id, card.id, created.id)
            .await?;
        assert_eq!(redownloaded, payload_v2);

        assert_stamp_advancement(
            client,
            board_id,
            stack.id,
            card.id,
            stamp_before,
            updated.last_modified,
            &validators,
        )
        .await?;

        soft_delete_and_restore(client, board_id, stack.id, card.id, created.id, &payload_v2)
            .await?;

        Ok(())
    }

    /// D1 §3.3: the attachment write advances the card's entity stamp to
    /// the attachment's own stamp. Poll under the tier deadline (Deck
    /// cache lag), then assert the measured equality; re-presenting the
    /// pre-write validators must yield fresh data, never a 304.
    async fn assert_stamp_advancement(
        client: &DeckClient,
        board_id: u64,
        stack_id: u64,
        card_id: u64,
        stamp_before: i64,
        attachment_stamp: i64,
        validators: &taskboard_sync_nextcloud::Validators,
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        let deadline = tokio::time::Instant::now() + super::LIVE_DEADLINE;
        let mut stamped = client.card(board_id, stack_id, card_id).await?;
        while stamped.last_modified < attachment_stamp {
            assert!(
                tokio::time::Instant::now() < deadline,
                "card stamp never advanced to the attachment's: card {:?} vs attachment {:?}",
                stamped.last_modified,
                attachment_stamp
            );
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            stamped = client.card(board_id, stack_id, card_id).await?;
        }
        // Second-granularity stamps (roundtrip §7's accepted blind spot):
        // on a warm server the whole write sequence can land inside one
        // wall-clock second, where "advanced" is unobservable — the
        // measured contract is the *equality* with the attachment's stamp
        // below plus the validator invalidation after it.
        assert!(
            stamped.last_modified >= stamp_before,
            "attachment writes must not move the card stamp backwards \
             (card {}, before {}, attachment {})",
            stamped.last_modified,
            stamp_before,
            attachment_stamp
        );
        assert_eq!(
            stamped.last_modified, attachment_stamp,
            "the new card stamp must equal the attachment's stamp"
        );

        let refetched = client
            .fetch_card(board_id, stack_id, card_id, validators)
            .await?;
        let fresh = refetched
            .data
            .unwrap_or_else(|| panic!("stale validators must not yield 304 after a write"));
        assert!(
            fresh.last_modified >= stamp_before,
            "refetched card must carry post-write data"
        );
        Ok(())
    }

    /// Soft delete (stamped; the tombstone stays in the listing) and
    /// restore (revived; the content survives the round trip).
    async fn soft_delete_and_restore(
        client: &DeckClient,
        board_id: u64,
        stack_id: u64,
        card_id: u64,
        attachment_id: u64,
        expected_content: &[u8],
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        let tombstone = super::retrying_server_errors(|| async {
            client
                .delete_attachment(board_id, stack_id, card_id, attachment_id)
                .await
        })
        .await?;
        assert_ne!(tombstone.deleted_at, 0, "delete must stamp deletedAt");
        let listing = client.attachments(board_id, stack_id, card_id).await?;
        assert!(
            listing.iter().any(|a| a.id == attachment_id),
            "soft-deleted attachment must stay listed (tombstone)"
        );

        let restored = super::retrying_server_errors(|| async {
            client
                .restore_attachment(board_id, stack_id, card_id, attachment_id)
                .await
        })
        .await?;
        assert_eq!(restored.deleted_at, 0);
        let after_restore = client
            .download_attachment(board_id, stack_id, card_id, attachment_id)
            .await?;
        assert_eq!(after_restore, expected_content);
        Ok(())
    }

    /// `startdate` read/write round trip (D-F2's one-assertion-per-direction
    /// wire pins, gated on the Deck 1.17.5 tier by DDF-10): create with a
    /// startdate reads it back; a full-send card PUT with a new startdate
    /// reads the new value back; a create without one reads `None`.
    pub(crate) async fn startdate_round_trip(client: &DeckClient) {
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid lifecycle color");

        let board = client
            .create_board(&run, &color)
            .await
            .expect("board creation must succeed");

        let result = startdate_body(client, board.id, &run).await;

        let deleted = client.delete_board(board.id).await;
        match (result, deleted) {
            (Ok(()), Ok(d)) => assert!(!d.is_live(), "DELETE payload must stamp deletedAt"),
            (Err(body_err), _) => panic!("startdate round trip failed: {body_err:?}"),
            (Ok(()), Err(teardown_err)) => panic!("board teardown failed: {teardown_err}"),
        }
    }

    async fn startdate_body(
        client: &DeckClient,
        board_id: u64,
        run: &str,
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        use chrono::SubsecRound as _;
        use taskboard_sync_nextcloud::NewCard;

        let stack = super::retrying_server_errors(|| async {
            client
                .create_stack(board_id, &format!("{run}-stack"), 0)
                .await
        })
        .await?;

        // Create with a startdate; the created value reads back equal at
        // second granularity (whole seconds are what we send).
        let start = (chrono::Utc::now() + chrono::Duration::days(3)).trunc_subsecs(0);
        let card = super::retrying_server_errors(|| async {
            client
                .create_card(
                    board_id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-started"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        startdate: Some(start),
                        kind: None,
                    },
                )
                .await
        })
        .await?;
        let read = client.card(board_id, stack.id, card.id).await?;
        assert_eq!(
            read.startdate,
            Some(start),
            "the created startdate must read back"
        );

        // Full-send card PUT with a new startdate; the new value reads back.
        let new_start = start + chrono::Duration::days(1);
        let mut fetched = client.card(board_id, stack.id, card.id).await?;
        fetched.startdate = Some(new_start);
        super::retrying_server_errors(|| async { client.update_card(board_id, &fetched).await })
            .await?;
        let updated = client.card(board_id, stack.id, card.id).await?;
        assert_eq!(
            updated.startdate,
            Some(new_start),
            "the PUT startdate must read back"
        );

        // Create without a startdate; `null` on the wire decodes `None`.
        let plain = super::retrying_server_errors(|| async {
            client
                .create_card(
                    board_id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-plain"),
                        order: Some(1),
                        description: None,
                        duedate: None,
                        startdate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await?;
        let plain_read = client.card(board_id, stack.id, plain.id).await?;
        assert_eq!(
            plain_read.startdate, None,
            "absent startdate must decode None"
        );
        Ok(())
    }

    /// The Deck 1.17.5 gate pin (DDF-10 as amended, characterization
    /// §2B): the server is the fixed party — create with a startdate
    /// answers 200 but ignores the field; a full-send card PUT with a
    /// valued startdate answers 200 and silently drops it (no 400, no
    /// effect). This is the capability table's measured evidence, not
    /// behavior debt of ours.
    pub(crate) async fn startdate_unsupported_on_deck1175(client: &DeckClient) {
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid lifecycle color");

        let board = client
            .create_board(&run, &color)
            .await
            .expect("board creation must succeed");

        let result = startdate_unsupported_body(client, board.id, &run).await;

        let deleted = client.delete_board(board.id).await;
        match (result, deleted) {
            (Ok(()), Ok(d)) => assert!(!d.is_live(), "DELETE payload must stamp deletedAt"),
            (Err(body_err), _) => panic!("startdate gate pin failed: {body_err:?}"),
            (Ok(()), Err(teardown_err)) => panic!("board teardown failed: {teardown_err}"),
        }
    }

    async fn startdate_unsupported_body(
        client: &DeckClient,
        board_id: u64,
        run: &str,
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        use chrono::SubsecRound as _;
        use taskboard_sync_nextcloud::NewCard;

        let stack = super::retrying_server_errors(|| async {
            client
                .create_stack(board_id, &format!("{run}-stack"), 0)
                .await
        })
        .await?;

        let start = (chrono::Utc::now() + chrono::Duration::days(3)).trunc_subsecs(0);
        // Create with a startdate: 200, but the field is ignored.
        let card = super::retrying_server_errors(|| async {
            client
                .create_card(
                    board_id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-started"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        startdate: Some(start),
                        kind: None,
                    },
                )
                .await
        })
        .await?;
        let read = client.card(board_id, stack.id, card.id).await?;
        assert_eq!(
            read.startdate, None,
            "1.17.5 create must ignore the startdate field"
        );

        // Full-send card PUT with a valued startdate: 200, silent drop.
        let mut fetched = client.card(board_id, stack.id, card.id).await?;
        fetched.startdate = Some(start);
        super::retrying_server_errors(|| async { client.update_card(board_id, &fetched).await })
            .await?;
        let updated = client.card(board_id, stack.id, card.id).await?;
        assert_eq!(
            updated.startdate, None,
            "1.17.5 must silently drop the PUT startdate (no 400, no effect)"
        );
        Ok(())
    }

    /// The D-V1 seam pinned end-to-end against a real server: the
    /// capabilities probe decodes the Deck version, the decoded version
    /// meets the tier's measured floor, and the derived flag matches the
    /// tier's measured `card_startdate` row. `min_version` is a
    /// `(major, minor, patch)` floor — not an exact pin — so image
    /// refreshes within the measured generation (patch and minor drift)
    /// survive while a regression below the measured generation fails.
    pub(crate) async fn capabilities_probe(
        client: &DeckClient,
        min_version: (u64, u64, u64),
        expect_startdate: bool,
    ) {
        let caps = client
            .capabilities()
            .await
            .expect("the capabilities probe must succeed against a real server");
        let decoded = parse_deck_version(
            caps.deck_version
                .as_deref()
                .expect("the probe must decode the deck section's version"),
        )
        .expect("the server-reported Deck version must decode");
        assert!(
            decoded >= min_version,
            "the tier's Deck version must meet the measured floor: \
             decoded {decoded:?} < floor {min_version:?}"
        );
        let version = caps.deck_version.expect("checked above");
        let features = taskboard_domain::flags_for(&version);
        assert_eq!(
            features.card_startdate, expect_startdate,
            "the derived flag must match the tier's pinned capability row"
        );
    }

    /// Parses a server-reported Deck version into `(major, minor, patch)`
    /// for floor comparison, mirroring the domain's documented leniency
    /// (`parse_major_minor` + `leading_digits`): a leading-digits prefix
    /// per component, any suffix ignored, `major.minor` required and
    /// patch defaulting to 0 when absent — a future `"1.20"`-style or
    /// suffix-bearing report must still decode, exactly as the
    /// production derivation tolerates it. `None` (missing or
    /// non-numeric major/minor) is a probe-contract failure.
    fn parse_deck_version(version: &str) -> Option<(u64, u64, u64)> {
        let mut parts = version.split('.');
        let major = leading_digits(parts.next()?)?;
        let minor = leading_digits(parts.next()?)?;
        let patch = parts.next().map_or(Some(0), leading_digits)?;
        Some((major, minor, patch))
    }

    /// The digits prefix of one version component (`"19"` → 19, `"5beta"`
    /// → 5, `""`/`"beta"` → `None`).
    fn leading_digits(part: &str) -> Option<u64> {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    }

    /// Deleting a missing board yields a typed error. Nextcloud 35 answers
    /// Forbidden (not `NotFound`) for boards that do not exist or are not
    /// ours; a plausible-but-absent id is used because huge ids overflow
    /// Deck's int columns and yield a 500.
    pub(crate) async fn delete_missing_board_is_typed_error(client: &DeckClient) {
        let err = client.delete_board(424_242).await.unwrap_err();
        assert!(
            matches!(err, DeckError::NotFound | DeckError::Forbidden),
            "unexpected error variant: {err:?}"
        );
    }

    /// The below-floor branch of `parse_deck_version` + the tuple
    /// compare is unexercisable through any real tier (servers only
    /// report current versions — precisely what broke the old literal
    /// pin), so this unit is the only layer that can hold it.
    #[cfg(test)]
    mod version_floor {
        use super::parse_deck_version;

        #[test]
        fn decodes_with_patch_leniency_and_compares() {
            assert_eq!(parse_deck_version("1.19.1"), Some((1, 19, 1)));
            // Two-component report: patch defaults to 0.
            assert_eq!(parse_deck_version("1.20"), Some((1, 20, 0)));
            // Suffix-bearing patch component decodes to its digit prefix.
            assert_eq!(parse_deck_version("1.19.0rc3"), Some((1, 19, 0)));
            // At-floor and patch-above pass; minor-below fails.
            let floor = (1, 19, 0);
            assert!(parse_deck_version("1.19.0").unwrap() >= floor);
            assert!(parse_deck_version("1.19.2").unwrap() >= floor);
            assert!(parse_deck_version("1.18.9").unwrap() < floor);
            // Non-decodable reports are None (probe-contract failure).
            assert_eq!(parse_deck_version(""), None);
            assert_eq!(parse_deck_version("deck"), None);
        }
    }
}

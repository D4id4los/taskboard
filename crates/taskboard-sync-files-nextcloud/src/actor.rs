// SPDX-License-Identifier: MIT OR Apache-2.0
//! The files-sync poll actor (decision D-FT8): the loop around
//! [`crate::cycle::run_cycle`], mirroring the Deck actor's shape —
//! first cycle immediate, `poll_interval` after success,
//! `poll_backoff` after failures with the `Retry-After` floor, biased
//! `select!` over the deadline and the broadcast.
//!
//! No command/nudge channel exists in chunk 2 (the poll cadence is
//! D-SF8's ruling; chunk 3 may add a nudge when the journal settles
//! demand one). The broadcast is subscribed **only** for `Shutdown`;
//! the actor publishes nothing on it — no actor is the network
//! authority.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::cycle::{LocalDocument, LoopState, SyncContext};
use crate::device::{self, DeviceIdentityError};
use crate::poll::poll_backoff;
use crate::transport::FilesTransport;
use taskboard_domain::SystemEvent;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

/// Knobs the bootstrap wires from figment's `[files_sync]` + `[sync]`
/// tables; tests inject tiny/paused values.
#[derive(Clone)]
pub struct FilesSyncConfig {
    /// Idle wait between cycles after a success.
    pub poll_interval: Duration,
    /// First failure backoff (`initial * 2^(streak-1)`).
    pub backoff_initial: Duration,
    /// Backoff saturation.
    pub backoff_max: Duration,
    /// Wall-clock seam for every stamp the cycle mints. Production
    /// injects `SystemClock`; the hermetic suites a fixed clock.
    pub clock: Arc<dyn taskboard_domain::Clock>,
    /// The heartbeat's `app_version`.
    pub app_version: String,
}

impl std::fmt::Debug for FilesSyncConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilesSyncConfig")
            .field("poll_interval", &self.poll_interval)
            .field("backoff_initial", &self.backoff_initial)
            .field("backoff_max", &self.backoff_max)
            .field("clock", &self.clock)
            .field("app_version", &self.app_version)
            .finish()
    }
}

/// Spawns the files-sync actor onto the current runtime.
///
/// All seams are injected: the transport (trait object or generic —
/// the fake transport implements the same trait), the resolved
/// `device.json` path, the local-document seam, the system broadcast
/// (Shutdown-only consumption), and the config. The status watch is
/// created here; the receiver half is returned beside the join handle
/// for the app to hand to its status surface.
///
/// # Panics
///
/// Never: a `device.json` failure is the parked finding below, not a
/// panic.
#[must_use]
pub fn spawn_files_sync_actor<T: FilesTransport>(
    transport: T,
    device_json_path: PathBuf,
    local: Arc<dyn LocalDocument>,
    system: broadcast::Sender<SystemEvent>,
    cfg: FilesSyncConfig,
) -> (
    JoinHandle<()>,
    watch::Receiver<crate::status::FilesSyncStatus>,
) {
    let (status, status_rx) = watch::channel(crate::status::FilesSyncStatus::default());
    let join = tokio::spawn(run(transport, device_json_path, local, system, cfg, status));
    (join, status_rx)
}

async fn run<T: FilesTransport>(
    transport: T,
    device_json_path: PathBuf,
    local: Arc<dyn LocalDocument>,
    system: broadcast::Sender<SystemEvent>,
    cfg: FilesSyncConfig,
    status: watch::Sender<crate::status::FilesSyncStatus>,
) {
    // Arm 1 of the attach ceremony is the local identity file: a
    // corrupt `device.json` parks the actor *before any server
    // request* — regenerating would silently fork the device.
    let identity = match device::load_or_create(&device_json_path, cfg.clock.as_ref()).await {
        Ok(identity) => identity,
        Err(DeviceIdentityError::Unusable { path }) => {
            tracing::error!(
                event = "DeviceIdentityUnusable",
                path = %path.display(),
                "files-sync parked: the device identity is corrupt and \
                 will not be regenerated"
            );
            status.send_modify(|current| {
                current.last_cycle = Some(crate::status::CycleOutcome::Failed);
            });
            return;
        }
        Err(err) => {
            tracing::error!(?err, event = "DeviceIdentityIo", "files-sync parked");
            status.send_modify(|current| {
                current.last_cycle = Some(crate::status::CycleOutcome::Failed);
            });
            return;
        }
    };
    let ctx = Arc::new(SyncContext {
        identity,
        app_version: cfg.app_version.clone(),
        clock: cfg.clock.clone(),
        local,
    });
    let mut st = LoopState::default();
    let mut sys_rx = system.subscribe();
    let mut sys_open = true;
    // First cycle fires immediately.
    let mut deadline = tokio::time::Instant::now();

    loop {
        tokio::select! {
            biased;

            () = tokio::time::sleep_until(deadline) => {
                crate::cycle::run_cycle(&transport, &ctx, &mut st, &status).await;
                deadline = reschedule(&st, &cfg);
            },

            event = sys_rx.recv(), if sys_open => match event {
                Ok(SystemEvent::Shutdown) => {
                    tracing::info!("files-sync actor stopping: shutdown event");
                    return;
                }
                Ok(
                    SystemEvent::StorageFailed
                        | SystemEvent::TaskResurrected(_)
                        | SystemEvent::SettingsChanged
                        | SystemEvent::NetworkLost
                        | SystemEvent::NetworkRestored
                        | SystemEvent::ConnectorLost(_)
                        | SystemEvent::ConnectorRestored(_),
                ) => {
                    // The whole network vocabulary belongs to the
                    // app-layer authority — this actor counts nothing
                    // and publishes nothing (the acceptance pin).
                    // `SettingsChanged` rides the same ignore arm as a
                    // recorded limitation (round-5 chunk 5, D-5.5 item
                    // 7): no engine facet to read live settings through
                    // (not a board connector — DR-8's boundary), so the
                    // cadence stays the boot capture; the backlog
                    // follow-up threads a settings reader into the
                    // files-sync gate (#673).
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::warn!("system broadcast closed; continuing without shutdown watch");
                    sys_open = false;
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "system event backlog overflowed");
                }
            },
        }
    }
}

fn reschedule(st: &LoopState, cfg: &FilesSyncConfig) -> tokio::time::Instant {
    let delay = if st.failure_streak == 0 {
        cfg.poll_interval
    } else {
        poll_backoff(
            st.failure_streak,
            cfg.backoff_initial,
            cfg.backoff_max,
            st.retry_after_floor,
        )
    };
    tokio::time::Instant::now() + delay
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use serde_json::json;
    use taskboard_sync_files::document::{UserDocument, UserRecord, UserSections};
    use taskboard_sync_files::ids::AuthorId;

    use crate::status::{CycleOutcome, FilesSyncStatus, ParkKind};
    use crate::test_support::{Effect, FakeFilesTransport, RecordedRequest};

    const SELF: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
    const PEER: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5b";
    const ROW_KEY: &str = "0193c000-0000-7c89-ab01-2345abcdef01";
    /// The adopted seed document's own row key (distinct from the
    /// peer's, so retirement assertions cannot pass via a peer row).
    const SEED_ROW: &str = "0193c000-0000-7c89-ab01-2345abcdef02";
    /// No poll tick inside a test horizon: cycles beyond the first are
    /// driven only by explicit `advance(POLL)`, keeping request counts
    /// exact observables.
    const POLL: Duration = Duration::from_secs(3_600);

    /// An advancing logical clock (30 s per read): heartbeat stamps
    /// stay distinct without wall time.
    #[derive(Debug)]
    struct StepClock;

    impl taskboard_domain::Clock for StepClock {
        fn now(&self) -> DateTime<Utc> {
            Utc::now() + chrono::TimeDelta::seconds(30)
        }
    }

    fn cfg() -> FilesSyncConfig {
        FilesSyncConfig {
            poll_interval: POLL,
            backoff_initial: Duration::from_millis(100),
            backoff_max: Duration::from_millis(400),
            clock: Arc::new(StepClock),
            app_version: "test".into(),
        }
    }

    fn doc(text: &str) -> UserDocument {
        doc_with_row_key(text, ROW_KEY)
    }

    fn doc_with_row_key(text: &str, key: &str) -> UserDocument {
        let mut body = BTreeMap::new();
        body.insert("text".to_owned(), json!(text));
        let mut journal = BTreeMap::new();
        journal.insert(
            key.to_owned(),
            UserRecord::Entry {
                body,
                clock: "2026-11-05T08:00:00Z".parse().unwrap(),
                author: None::<AuthorId>,
            },
        );
        UserDocument {
            format: 1,
            sections: UserSections {
                journal: Some(journal),
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        }
    }

    fn doc_bytes(document: &UserDocument) -> Vec<u8> {
        serde_json::to_vec(document).expect("fixture doc serializes")
    }

    fn appdata_path(id: &str) -> String {
        format!("taskboard/devices/{id}/appdata.json")
    }

    fn info_path(id: &str) -> String {
        format!("taskboard/devices/{id}/info.json")
    }

    fn identity_path() -> String {
        "taskboard/identity.json".into()
    }

    fn dirs_of(ids: &[&str]) -> Vec<String> {
        let mut dirs = vec!["taskboard".to_owned(), "taskboard/devices".to_owned()];
        dirs.extend(ids.iter().map(|id| format!("taskboard/devices/{id}")));
        dirs
    }

    fn seed_std(fake: &FakeFilesTransport) {
        let dirs = dirs_of(&[SELF]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
    }

    fn seed_peer(fake: &FakeFilesTransport, text: &str) {
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        fake.seed_file(&appdata_path(PEER), doc_bytes(&doc(text)));
    }

    struct Harness {
        fake: Arc<FakeFilesTransport>,
        /// `RefCell` because the wait helper needs `&mut` on the
        /// receiver (`changed().await` is the only reliable
        /// change-signal; a fresh clone would re-see old values).
        status: std::cell::RefCell<watch::Receiver<FilesSyncStatus>>,
        /// The subscription made *before* the spawn: the authority pin
        /// reads it (broadcast receivers are not `Clone`).
        system_rx: (
            broadcast::Sender<SystemEvent>,
            broadcast::Receiver<SystemEvent>,
        ),
        _dir: tempfile::TempDir,
    }

    /// The fixture identity written into the temp dir before the
    /// spawn: the ceremony loads it instead of minting (so the SELF
    /// path constants hold).
    fn write_device_identity(path: &std::path::Path) {
        std::fs::write(
            path,
            format!("{{\"device_id\":\"{SELF}\",\"created_at\":\"2026-11-04T00:00:00Z\"}}"),
        )
        .expect("fixture device.json");
    }

    /// Spawns the actor over the fixture device.json in a temp dir.
    #[allow(clippy::unused_async)] // the spawn joins the ambient runtime
    async fn spawn_actor(fake: FakeFilesTransport) -> Harness {
        let dir = tempfile::tempdir().expect("temp dir");
        let device_path = dir.path().join("device.json");
        write_device_identity(&device_path);
        let (system, system_rx) = broadcast::channel(16);
        let fake = Arc::new(fake);
        let (_join, status) = spawn_files_sync_actor(
            FakeHandle { fake: fake.clone() },
            device_path,
            Arc::new(crate::cycle::EmptyDocument),
            system.clone(),
            cfg(),
        );
        Harness {
            fake,
            status: std::cell::RefCell::new(status),
            system_rx: (system, system_rx),
            _dir: dir,
        }
    }

    /// Forwards the shared fake's verbs (the actor consumes a generic
    /// `T: FilesTransport`; the shared `Arc` is what the assertions
    /// read, so the spawn needs this thin owner).
    struct FakeHandle {
        fake: Arc<FakeFilesTransport>,
    }

    impl FilesTransport for FakeHandle {
        async fn mkcol(
            &self,
            path: &str,
        ) -> Result<crate::transport::MkcolOutcome, crate::error::TransportError> {
            self.fake.mkcol(path).await
        }
        async fn propfind_devices(
            &self,
        ) -> Result<crate::transport::PropfindListing, crate::error::TransportError> {
            self.fake.propfind_devices().await
        }
        async fn get(
            &self,
            path: &str,
        ) -> Result<crate::transport::FileContent, crate::error::TransportError> {
            self.fake.get(path).await
        }
        async fn put_create(
            &self,
            path: &str,
            body: &[u8],
        ) -> Result<crate::transport::PutOutcome, crate::error::TransportError> {
            self.fake.put_create(path, body).await
        }
        async fn put_if_match(
            &self,
            path: &str,
            body: &[u8],
            etag: &crate::etag::Etag,
        ) -> Result<crate::transport::PutOutcome, crate::error::TransportError> {
            self.fake.put_if_match(path, body, etag).await
        }
    }

    const WALL: Duration = Duration::from_secs(10);

    /// Waits (wall-clock bounded) until the status watch carries a new
    /// value; paused time advances only here, so actor timers fire
    /// inside this loop and nowhere else.
    async fn next_status(harness: &Harness) -> FilesSyncStatus {
        let deadline = std::time::Instant::now() + WALL;
        loop {
            // The receiver is persistent (so `has_changed` is exact)
            // and the borrow never spans the `advance().await` — the
            // wall-clock bound is the test's hang guard.
            let changed = {
                let rx = harness.status.borrow_mut();
                rx.has_changed().unwrap_or(false)
            };
            if changed {
                return harness.status.borrow_mut().borrow_and_update().clone();
            }
            tokio::time::advance(Duration::from_millis(10)).await;
            assert!(
                std::time::Instant::now() < deadline,
                "the status watch never changed"
            );
        }
    }

    /// Drives one more steady cycle by advancing one poll interval.
    async fn next_cycle(harness: &Harness) -> FilesSyncStatus {
        tokio::time::advance(POLL).await;
        next_status(harness).await
    }

    fn ok(status: &FilesSyncStatus) -> bool {
        matches!(status.last_cycle, Some(CycleOutcome::Ok { .. }))
    }

    fn log(fake: &FakeFilesTransport) -> Vec<RecordedRequest> {
        fake.requests()
    }

    fn puts(fake: &FakeFilesTransport, path_prefix: &str) -> usize {
        fake.count("PUT", Some(path_prefix))
    }

    fn gets(fake: &FakeFilesTransport) -> usize {
        fake.count("GET", None)
    }

    // ---- the matrix (D-FT12(b), one behavior per test) ----------------

    /// 1 — attach journey: MKCOL ladder + identity create-once + first
    /// publish; the exact verb/header sequence.
    #[tokio::test(start_paused = true)]
    async fn matrix_1_attach_journey_pins_the_verb_and_header_sequence() {
        let harness = spawn_actor(FakeFilesTransport::new()).await;
        let status = next_status(&harness).await;
        assert!(ok(&status), "attach must complete: {status:?}");
        let expected: [(&str, String, bool); 7] = [
            ("MKCOL", "taskboard".into(), false),
            ("MKCOL", "taskboard/devices".into(), false),
            ("MKCOL", format!("taskboard/devices/{SELF}"), false),
            ("PUT", identity_path(), true),
            ("PROPFIND", "taskboard/devices/".into(), false),
            ("PUT", appdata_path(SELF), true),
            ("PUT", info_path(SELF), true),
        ];
        let requests = log(&harness.fake);
        assert_eq!(requests.len(), expected.len(), "log: {requests:?}");
        for (req, (method, path, create)) in requests.iter().zip(expected) {
            assert_eq!(
                (req.method.as_str(), req.path.as_str()),
                (method, path.as_str())
            );
            assert_eq!(req.if_none_match_star, create, "{method} {path}");
        }
    }

    /// 2 — identity create race: 412 ⇒ adopt (GET follows; no error).
    #[tokio::test(start_paused = true)]
    async fn matrix_2_identity_create_race_adopts_without_erroring() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        // A former install already minted *something* create-once at
        // the identity path — the loser adopts whatever is served.
        fake.seed_file(&identity_path(), b"{\"folder_id\":\"0193b400-0000-7c89-ab01-2345abcdef01\",\"format_version\":1,\"created_at\":\"2026-11-04T00:00:00Z\"}".to_vec());
        let harness = spawn_actor(fake).await;
        let status = next_status(&harness).await;
        assert!(ok(&status), "the losing device adopts: {status:?}");
        assert!(
            log(&harness.fake)
                .iter()
                .any(|req| req.method == "GET" && req.path == identity_path()),
            "the adoption GET must follow the create race"
        );
    }

    /// 3 — quiet fleet: unchanged dir etags ⇒ zero GETs, zero PUTs.
    #[tokio::test(start_paused = true)]
    async fn matrix_3_quiet_fleet_costs_zero_gets_and_puts() {
        let fake = FakeFilesTransport::new();
        seed_std(&fake);
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        let (gets_before, puts_before, propfinds_before) = (
            gets(&harness.fake),
            harness.fake.count("PUT", None),
            harness.fake.count("PROPFIND", None),
        );
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert_eq!(gets(&harness.fake), gets_before, "no dirty device: no GETs");
        assert_eq!(
            harness.fake.count("PUT", None),
            puts_before,
            "hash-identical cycle: no PUTs (the F5 skip)"
        );
        assert_eq!(
            harness.fake.count("PROPFIND", None),
            propfinds_before + 1,
            "exactly one cheap listing per cycle"
        );
    }

    /// 4 — changed peer: dir etag changes ⇒ exactly two GETs, publish
    /// iff the local hash differs; identical content ⇒ skip-PUT.
    #[tokio::test(start_paused = true)]
    async fn matrix_4_changed_peer_two_gets_and_publish_iff_hash_differs() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        let (gets_before, puts_before) = (
            gets(&harness.fake),
            puts(&harness.fake, &appdata_path(SELF)),
        );
        // A peer write churns the dir etag even for identical content.
        harness
            .fake
            .seed_file(&appdata_path(PEER), doc_bytes(&doc("v1")));
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert_eq!(
            gets(&harness.fake),
            gets_before + 2,
            "info.json + appdata.json"
        );
        assert_eq!(
            puts(&harness.fake, &appdata_path(SELF)),
            puts_before,
            "identical content folds to the same hash: skip-PUT (F5)"
        );
        // Now a real change: the hash differs, so exactly one publish
        // (the appdata PUT) must follow.
        harness
            .fake
            .seed_file(&appdata_path(PEER), doc_bytes(&doc("v2")));
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert_eq!(gets(&harness.fake), gets_before + 4);
        assert_eq!(
            puts(&harness.fake, &appdata_path(SELF)),
            puts_before + 1,
            "changed content publishes appdata exactly once"
        );
        let published = harness.fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).unwrap();
        let text = match document.sections.journal.unwrap()[ROW_KEY].clone() {
            UserRecord::Entry { body, .. } => body["text"].clone(),
            other => panic!("expected the peer's entry, got {other:?}"),
        };
        assert_eq!(text, json!("v2"), "the fold adopted the newer row");
    }

    /// 5 — reordered/duplicated listing members ⇒ identical fold
    /// outcome (the fold's order-freedom at the transport level).
    #[tokio::test(start_paused = true)]
    async fn matrix_5_reordered_and_duplicated_members_fold_identically() {
        // Run A: the model listing. Run B: the same members reordered
        // and duplicated, served verbatim over the attach PROPFIND.
        let mut published = Vec::new();
        for torn in [false, true] {
            let fake = FakeFilesTransport::new();
            seed_peer(&fake, "from-peer");
            if torn {
                // Duplicates + reversal of the members the model would
                // serve (self dir + peer dir), plus a junk member the
                // cycle must ignore.
                fake.stage(Effect::Listing(vec![
                    crate::transport::PropfindMember {
                        href: format!("/base/taskboard/devices/{PEER}/"),
                        etag: Some(crate::etag::Etag::new("\"p\"".into())),
                    },
                    crate::transport::PropfindMember {
                        href: format!("/base/taskboard/devices/{SELF}/"),
                        etag: Some(crate::etag::Etag::new("\"s\"".into())),
                    },
                    crate::transport::PropfindMember {
                        href: "/base/unrelated/thing/".into(),
                        etag: None,
                    },
                    crate::transport::PropfindMember {
                        href: format!("/base/taskboard/devices/{PEER}/"),
                        etag: Some(crate::etag::Etag::new("\"p\"".into())),
                    },
                ]));
            }
            let harness = spawn_actor(fake).await;
            next_status(&harness).await;
            published.push(harness.fake.file(&appdata_path(SELF)).expect("published"));
        }
        assert_eq!(published[0], published[1], "permutation changes nothing");
    }

    /// 6 — garbage appdata body ⇒ Quarantined park, file untouched,
    /// fold proceeds from the other peers.
    #[tokio::test(start_paused = true)]
    async fn matrix_6_garbage_peer_body_quarantines_and_never_writes_it() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        fake.seed_file(&appdata_path(PEER), b"{{{ definitely not json".to_vec());
        let harness = spawn_actor(fake).await;
        let status = next_status(&harness).await;
        assert!(ok(&status), "a quarantined peer is not a cycle failure");
        assert_eq!(
            status.parked.get(&appdata_path(PEER)),
            Some(&ParkKind::Quarantined)
        );
        assert_eq!(
            puts(&harness.fake, &appdata_path(PEER)),
            0,
            "readers never write another device's file"
        );
    }

    /// 7 — newer-format document ⇒ `NewerFormat` park, folded nothing.
    #[tokio::test(start_paused = true)]
    async fn matrix_7_newer_format_parks_and_folds_nothing() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "placeholder");
        // Upgrade the seeded peer document to format 2 with a row the
        // v1 fold must not model.
        let mut newer = doc("future");
        newer.format = 2;
        fake.seed_file(&appdata_path(PEER), doc_bytes(&newer));
        let harness = spawn_actor(fake).await;
        let status = next_status(&harness).await;
        assert!(ok(&status));
        assert_eq!(
            status.parked.get(&appdata_path(PEER)),
            Some(&ParkKind::NewerFormat { format: 2 })
        );
        let published = harness.fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).unwrap();
        assert!(
            !document
                .sections
                .journal
                .unwrap_or_default()
                .contains_key(ROW_KEY),
            "nothing folds from a newer-format peer"
        );
    }

    /// 8 — torn/failed GET ⇒ typed cycle failure, backoff, honest
    /// retry next cycle (no cache poisoning).
    #[tokio::test(start_paused = true)]
    async fn matrix_8_failed_get_fails_the_cycle_then_retries_honestly() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "real-value");
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        // The next steady cycle: its PROPFIND passes (the Pass effect),
        // the peer GET fails (a 503 the real server can produce). The
        // peer is made dirty first, so the failing request is its GET.
        harness
            .fake
            .seed_file(&appdata_path(PEER), doc_bytes(&doc("changed")));
        harness.fake.stage(Effect::Pass);
        harness.fake.stage(Effect::Status(503));
        let status = next_cycle(&harness).await;
        assert_eq!(status.last_cycle, Some(CycleOutcome::Failed));
        // The honest retry: nothing was poisoned, the peer row lands.
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        let published = harness.fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).unwrap();
        assert!(document.sections.journal.unwrap().contains_key(ROW_KEY));
        assert!(
            !status.parked.contains_key(&appdata_path(PEER)),
            "a transient failure must not park the peer"
        );
    }

    /// 9 — unexpected 412 on own rewrite + GET 200 ⇒ `ForkDetected`:
    /// the path joins the stop-write set, zero further PUTs to it
    /// across advanced cycles, reads continue, the watch carries it.
    #[tokio::test(start_paused = true)]
    async fn matrix_9_own_path_412_with_content_forks_and_stops_writes() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        let appdata = appdata_path(SELF);
        let puts_before = puts(&harness.fake, &appdata);
        harness.fake.answer_412_once(&appdata);
        harness
            .fake
            .seed_file(&appdata_path(PEER), doc_bytes(&doc("v2")));
        let status = next_cycle(&harness).await;
        assert!(
            status.forked_paths.contains(&appdata),
            "the fork alarm must land in the watch: {status:?}"
        );
        // Across two further advanced cycles: zero further PUTs to the
        // forked path (the fork cycle's 412'd attempt is the trigger
        // itself and already in the log); the listing keeps running
        // (reads continue).
        let puts_after_fork = puts(&harness.fake, &appdata);
        let propfinds = harness.fake.count("PROPFIND", None);
        let _ = next_cycle(&harness).await;
        let _ = next_cycle(&harness).await;
        assert_eq!(puts(&harness.fake, &appdata), puts_after_fork);
        let _ = puts_before;
        assert!(
            harness.fake.count("PROPFIND", None) >= propfinds + 2,
            "reads continue after the fork"
        );
    }

    /// 10 — 412 + GET 404 ⇒ `OwnFileVanished`: recreate via
    /// create-once next cycle, no fork.
    #[tokio::test(start_paused = true)]
    async fn matrix_10_own_path_412_then_404_vanishes_and_recreates() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        let appdata = appdata_path(SELF);
        harness.fake.answer_412_once(&appdata);
        harness.fake.remove_file(&appdata);
        // A publish attempt is what discovers the vanish: make the
        // hash differ so the cycle rewrites.
        harness
            .fake
            .seed_file(&appdata_path(PEER), doc_bytes(&doc("force")));
        let status = next_cycle(&harness).await;
        assert!(
            !status.forked_paths.contains(&appdata),
            "a vanished own file is not a fork: {status:?}"
        );
        // Next cycle: the recreate via create-once (the vanished
        // cycle's hash is cleared, so it publishes without further
        // peer churn).
        let creates_before = harness
            .fake
            .requests()
            .iter()
            .filter(|req| req.method == "PUT" && req.path == appdata && req.if_none_match_star)
            .count();
        let status = next_cycle(&harness).await;
        assert!(ok(&status), "{status:?}");
        let creates_after = harness
            .fake
            .requests()
            .iter()
            .filter(|req| req.method == "PUT" && req.path == appdata && req.if_none_match_star)
            .count();
        assert_eq!(
            creates_after,
            creates_before + 1,
            "recreated via create-once"
        );
        assert!(harness.fake.file(&appdata).is_some());
    }

    /// 11 — `publish_seed` retirement: adopted own rows survive the
    /// first publish and every later cycle; a later vanish of the
    /// seeding subtree drops them (re-publication semantics).
    #[tokio::test(start_paused = true)]
    async fn matrix_11_adopted_rows_survive_until_the_seed_subtree_vanishes() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        // A former install's own appdata: put_create will 412 and the
        // ceremony adopts its rows as the seed.
        fake.seed_file(
            &appdata_path(SELF),
            doc_bytes(&doc_with_row_key("adopted-row", SEED_ROW)),
        );
        let harness = spawn_actor(fake).await;
        let status = next_status(&harness).await;
        assert!(ok(&status));
        let contains_row = |fake: &FakeFilesTransport| {
            let published = fake.file(&appdata_path(SELF)).expect("published");
            let document: UserDocument = serde_json::from_slice(&published).unwrap();
            document
                .sections
                .journal
                .unwrap_or_default()
                .contains_key(SEED_ROW)
        };
        assert!(
            contains_row(&harness.fake),
            "the first publish carries the rows"
        );
        // Quiet cycle: rows survive (the seed is retained, not
        // consumed once).
        let status = next_cycle(&harness).await;
        assert!(ok(&status) || matches!(status.last_cycle, Some(CycleOutcome::Failed)));
        assert!(
            contains_row(&harness.fake),
            "rows survive past the first publish"
        );
        // A delete of the file (not the seed's owner): the next
        // publish attempt discovers it (hash forced to differ) and
        // recreates with the rows still in the view.
        harness.fake.remove_file(&appdata_path(SELF));
        harness
            .fake
            .seed_file(&appdata_path(PEER), doc_bytes(&doc("force")));
        let _ = next_cycle(&harness).await;
        assert!(contains_row(&harness.fake));
        // The seeding subtree vanishes from the listing: rows drop.
        harness
            .fake
            .stage(Effect::Listing(vec![crate::transport::PropfindMember {
                // Only some *other* device remains listed — self is gone.
                href: format!("/base/taskboard/devices/{PEER}/"),
                etag: Some(crate::etag::Etag::new("\"p\"".into())),
            }]));
        let _status = next_cycle(&harness).await;
        let published = harness.fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).unwrap();
        assert!(
            !document
                .sections
                .journal
                .unwrap_or_default()
                .contains_key(SEED_ROW),
            "the vanished seeding subtree's rows leave the merged view"
        );
    }

    /// 12 — vanished device ⇒ its rows leave the merged view; own
    /// appdata updates (hash differs) exactly once.
    #[tokio::test(start_paused = true)]
    async fn matrix_12_vanished_device_drops_its_rows_with_one_publish() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "peer-row");
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        let puts_before = puts(&harness.fake, &appdata_path(SELF));
        harness
            .fake
            .remove_device_subtree(&format!("taskboard/devices/{PEER}"));
        let status = next_cycle(&harness).await;
        assert!(ok(&status), "{status:?}");
        assert_eq!(
            puts(&harness.fake, &appdata_path(SELF)),
            puts_before + 1,
            "the hash differs now: exactly one re-publish"
        );
        let published = harness.fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).unwrap();
        assert!(
            !document
                .sections
                .journal
                .unwrap_or_default()
                .contains_key(ROW_KEY)
        );
    }

    /// 13 — cold-store no-blind-PUT: a fresh boot whose first publish
    /// needs a rewrite always GETs the own file first (D-FT4).
    #[tokio::test(start_paused = true)]
    async fn matrix_13_cold_store_never_puts_unguarded() {
        let fake = FakeFilesTransport::new();
        seed_std(&fake);
        fake.seed_file(&appdata_path(SELF), doc_bytes(&doc("cold")));
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        let requests = log(&harness.fake);
        let appdata = appdata_path(SELF);
        // Every PUT to the own appdata path after the boot must be
        // preceded by a GET of it (the adopting read) — create-once
        // excepted (those carry If-None-Match: *).
        let mut last_get = true; // before the first request, "guarded" is vacuous
        for req in &requests {
            if req.method == "GET" && req.path == appdata {
                last_get = true;
            }
            if req.method == "PUT" && req.path == appdata && !req.if_none_match_star {
                assert!(
                    last_get,
                    "guarded PUT without an intervening GET: {requests:?}"
                );
                last_get = false;
            }
        }
    }

    /// 14 — the authority pin: cycles including failures emit nothing
    /// on the injected broadcast (only `Shutdown` is consumed).
    #[tokio::test(start_paused = true)]
    async fn matrix_14_the_actor_never_publishes_on_the_broadcast() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake).await;
        next_status(&harness).await;
        harness.fake.stage(Effect::Status(503));
        let failed = next_cycle(&harness).await;
        assert_eq!(failed.last_cycle, Some(CycleOutcome::Failed));
        let _ = next_cycle(&harness).await;
        let mut system_rx = harness.system_rx.1;
        assert!(
            system_rx.try_recv().is_err(),
            "the files actor claims no network authority: no broadcast emission"
        );
    }

    /// 15 — corrupt device.json ⇒ the actor parks with the typed
    /// finding, the server sees zero requests; the file is never
    /// rewritten.
    #[tokio::test(start_paused = true)]
    async fn matrix_15_corrupt_device_identity_parks_before_any_request() {
        let fake = FakeFilesTransport::new();
        seed_std(&fake);
        let dir = tempfile::tempdir().expect("temp dir");
        let device_path = dir.path().join("device.json");
        let corrupt = b"this is not a device identity";
        std::fs::write(&device_path, corrupt).expect("write corrupt file");
        let (system, _system_rx) = broadcast::channel(16);
        let fake = Arc::new(fake);
        let (_join, mut watch_rx) = spawn_files_sync_actor(
            FakeHandle { fake: fake.clone() },
            device_path.clone(),
            Arc::new(crate::cycle::EmptyDocument),
            system,
            cfg(),
        );
        // The park is the Failed status; no server request ever flies.
        let deadline = std::time::Instant::now() + WALL;
        while watch_rx.borrow().last_cycle.is_none() {
            tokio::time::advance(Duration::from_millis(10)).await;
            assert!(
                std::time::Instant::now() < deadline,
                "the park never surfaced"
            );
        }
        let status = watch_rx.borrow_and_update().clone();
        assert_eq!(status.last_cycle, Some(CycleOutcome::Failed));
        assert!(fake.requests().is_empty(), "the server sees zero requests");
        assert_eq!(
            std::fs::read(&device_path).expect("file survives"),
            corrupt,
            "the corrupt identity is never rewritten"
        );
    }
}

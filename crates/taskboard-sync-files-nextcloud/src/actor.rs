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
use taskboard_domain::messages::SyncReport;
use taskboard_domain::{SystemEvent, UserDataReport};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;

/// Knobs the bootstrap wires from figment's `[files_sync]` + `[sync]`
/// tables; tests inject tiny/paused values.
#[derive(Clone)]
pub struct FilesSyncConfig {
    /// This connector's config-minted tag (DF-3): the board lanes'
    /// ownership key (chunk 5).
    pub connector: taskboard_domain::ids::ConnectorId,
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
    /// Each Deck connector's server origin (the move adoption's match).
    pub deck_origins: std::collections::BTreeMap<taskboard_domain::ids::ConnectorId, String>,
}

impl std::fmt::Debug for FilesSyncConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilesSyncConfig")
            .field("deck_origins", &self.deck_origins)
            .field("poll_interval", &self.poll_interval)
            .field("backoff_initial", &self.backoff_initial)
            .field("backoff_max", &self.backoff_max)
            .field("clock", &self.clock)
            .field("connector", &self.connector)
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
#[allow(clippy::too_many_arguments)] // the injected-seams shape: transport, identity, local doc, state reader, two report lanes, system, config
pub fn spawn_files_sync_actor<T: FilesTransport>(
    transport: T,
    device_json_path: PathBuf,
    local: Arc<dyn LocalDocument>,
    state_reader: Arc<dyn taskboard_domain::persistence::SyncStateReader>,
    report_tx: mpsc::Sender<UserDataReport>,
    board_reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    cfg: FilesSyncConfig,
) -> (
    JoinHandle<()>,
    watch::Receiver<crate::status::FilesSyncStatus>,
) {
    let (status, status_rx) = watch::channel(crate::status::FilesSyncStatus::default());
    let join = tokio::spawn(run(
        transport,
        device_json_path,
        local,
        state_reader,
        report_tx,
        board_reports,
        system,
        cfg,
        status,
    ));
    (join, status_rx)
}

#[allow(clippy::too_many_arguments)] // the spawn's seams, verbatim
async fn run<T: FilesTransport>(
    transport: T,
    device_json_path: PathBuf,
    local: Arc<dyn LocalDocument>,
    state_reader: Arc<dyn taskboard_domain::persistence::SyncStateReader>,
    report_tx: mpsc::Sender<UserDataReport>,
    board_reports: mpsc::Sender<SyncReport>,
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
        report_tx,
        connector: cfg.connector.clone(),
        state_reader,
        board_reports,
        deck_origins: cfg.deck_origins.clone(),
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc as StdArc;

    use chrono::{DateTime, Utc};
    use serde_json::json;
    use taskboard_sync_files::document::{UserDocument, UserRecord, UserSections};
    use taskboard_sync_files::ids::AuthorId;

    use taskboard_sync_files::hash::StateHash;
    use taskboard_sync_files::ids::DeviceId;

    use crate::status::{CycleOutcome, FilesSyncStatus, ParkKind, PeerFreshness};
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

    /// The fixture base for minted heartbeat stamps: one hour after the
    /// seed documents' clock literal (`2026-11-05T08:00:00Z`), so every
    /// heartbeat compares newer than every fixture stamp.
    fn heartbeat_base() -> DateTime<Utc> {
        use chrono::TimeZone as _;
        chrono::Utc
            .with_ymd_and_hms(2026, 11, 5, 9, 0, 0)
            .single()
            .expect("in-range stamp")
    }

    /// A logical stepping clock — 30 s per read from a fixed base:
    /// heartbeat stamps stay distinct and strictly increasing without
    /// any wall read.
    #[derive(Debug)]
    struct SteppingClock {
        next_secs: std::sync::atomic::AtomicI64,
        step_secs: i64,
    }

    impl SteppingClock {
        fn new(start: DateTime<Utc>, step_secs: i64) -> Self {
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

    fn cfg() -> FilesSyncConfig {
        FilesSyncConfig {
            connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
            poll_interval: POLL,
            backoff_initial: Duration::from_millis(100),
            backoff_max: Duration::from_millis(400),
            clock: Arc::new(SteppingClock::new(heartbeat_base(), 30)),
            app_version: "test".into(),
            deck_origins: std::collections::BTreeMap::new(),
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
                settings: None,
                proofs: None,
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
        /// The user-data lane's receiving half (the engine stand-in).
        reports: std::cell::RefCell<mpsc::Receiver<UserDataReport>>,
        /// The board lanes' receiving half (the engine stand-in).
        #[allow(dead_code)]
        // the board lane's receiving half (unused by the per-user matrix)
        board_reports: std::cell::RefCell<mpsc::Receiver<SyncReport>>,
        /// The board lanes' state seam: an empty persisted state (no
        /// files-owned boards — the per-user matrix runs lane-free).
        #[allow(dead_code)]
        state: Arc<dyn taskboard_domain::persistence::SyncStateReader>,
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
    fn spawn_actor(fake: FakeFilesTransport) -> Harness {
        spawn_actor_with(fake, Arc::new(crate::cycle::EmptyDocument))
    }

    /// Spawns the actor over an explicit local-document seam (the
    /// journal tests stand in the engine with a `PublishedState`).
    fn spawn_actor_with(
        fake: FakeFilesTransport,
        local: Arc<dyn crate::cycle::LocalDocument>,
    ) -> Harness {
        spawn_actor_with_reports(fake, local, mpsc::channel(4))
    }

    /// Spawns with an explicit lane pair (the fingerprint tests keep
    /// the sender to drive further cycles).
    fn spawn_actor_with_reports(
        fake: FakeFilesTransport,
        local: Arc<dyn crate::cycle::LocalDocument>,
        lane: (mpsc::Sender<UserDataReport>, mpsc::Receiver<UserDataReport>),
    ) -> Harness {
        let (report_tx, report_rx) = lane;
        let (board_tx, board_rx) = mpsc::channel::<SyncReport>(4);
        let state: Arc<dyn taskboard_domain::persistence::SyncStateReader> =
            Arc::new(EmptyPersistedState);
        let dir = tempfile::tempdir().expect("temp dir");
        let device_path = dir.path().join("device.json");
        write_device_identity(&device_path);
        let (system, system_rx) = broadcast::channel(16);
        let fake = Arc::new(fake);
        let (_join, status) = spawn_files_sync_actor(
            FakeHandle { fake: fake.clone() },
            device_path,
            local,
            state.clone(),
            report_tx,
            board_tx,
            system.clone(),
            cfg(),
        );
        Harness {
            fake,
            status: std::cell::RefCell::new(status),
            reports: std::cell::RefCell::new(report_rx),
            board_reports: std::cell::RefCell::new(board_rx),
            state,
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
        async fn propfind_tree(
            &self,
            path: &str,
        ) -> Result<crate::transport::PropfindListing, crate::error::TransportError> {
            self.fake.propfind_tree(path).await
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

    /// Waits (wall-clock bounded) for the engine stand-in to receive a
    /// report, advancing paused time so the actor runs.
    async fn next_report(harness: &Harness) -> UserDataReport {
        let deadline = std::time::Instant::now() + WALL;
        loop {
            if let Ok(report) = harness.reports.borrow_mut().try_recv() {
                return report;
            }
            tokio::time::advance(Duration::from_millis(10)).await;
            assert!(
                std::time::Instant::now() < deadline,
                "no user-data report arrived"
            );
        }
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
        let harness = spawn_actor(FakeFilesTransport::new());
        let status = next_status(&harness).await;
        assert!(ok(&status), "attach must complete: {status:?}");
        // Chunk 5: the board lanes ride the same tick — the per-user
        // attach sequence, then the discovery scan's one cheap listing
        // (no boards dir content: an empty account discovers nothing).
        let expected: [(&str, String, bool); 8] = [
            ("MKCOL", "taskboard".into(), false),
            ("MKCOL", "taskboard/devices".into(), false),
            ("MKCOL", format!("taskboard/devices/{SELF}"), false),
            ("PUT", identity_path(), true),
            ("PROPFIND", "taskboard/devices/".into(), false),
            ("PUT", appdata_path(SELF), true),
            ("PUT", info_path(SELF), true),
            ("PROPFIND", "taskboard/boards/".into(), false),
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
            propfinds_before + 2,
            "one cheap listing per lane: the per-user devices listing plus the discovery scan"
        );
    }

    /// 4 — changed peer: dir etag changes ⇒ exactly two GETs, publish
    /// iff the local hash differs; identical content ⇒ skip-PUT.
    #[tokio::test(start_paused = true)]
    async fn matrix_4_changed_peer_two_gets_and_publish_iff_hash_differs() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake);
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
            let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let harness = spawn_actor(fake);
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
        let (report_tx, _report_rx) = mpsc::channel(4);
        let (board_tx, _board_rx) = mpsc::channel::<SyncReport>(4);
        let (_join, mut watch_rx) = spawn_files_sync_actor(
            FakeHandle { fake: fake.clone() },
            device_path.clone(),
            Arc::new(crate::cycle::EmptyDocument),
            Arc::new(EmptyPersistedState),
            report_tx,
            board_tx,
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

    // -----------------------------------------------------------------
    // X-series — the import step (DR-8 chunk 3, D-ES5/D-ES8): the
    // state-derived local document, the fold-first attach, the
    // fingerprint dedupe, the rejection counting. The engine is a
    // stand-in: an `Arc<ArcSwap<AppState>>` behind `PublishedState`
    // plus the lane's receiving half.
    // -----------------------------------------------------------------

    use arc_swap::ArcSwap;
    use taskboard_domain::reflection::{
        HealthFlag, Rating, ReflectionAttributes, ReflectionEntry, ReflectionState, ReviewCadence,
    };
    use taskboard_sync_files::journal::encode_row;

    use taskboard_domain::AppState;

    fn entry(id_raw: u128, text: &str, updated: chrono::DateTime<Utc>) -> ReflectionEntry {
        ReflectionEntry {
            id: taskboard_domain::ReflectionId::from(uuid::Uuid::from_u128(id_raw)),
            cadence: ReviewCadence::Day,
            day: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).expect("date"),
            window_start: "2026-11-05T00:00:00Z".parse().expect("instant"),
            window_end: "2026-11-06T00:00:00Z".parse().expect("instant"),
            text: text.to_owned(),
            attributes: ReflectionAttributes {
                mood: Some(Rating::try_new(3).expect("in range")),
                energy: None,
                flags: BTreeSet::from([HealthFlag::GoodDay]),
                extra_flags: BTreeSet::new(),
            },
            created_at: "2026-11-05T06:00:00Z".parse().expect("instant"),
            updated_at: updated,
            deleted: false,
        }
    }

    /// A peer document whose journal section is the encoded form of the
    /// given entries.
    fn journal_doc(entries: &[ReflectionEntry]) -> UserDocument {
        let rows: BTreeMap<String, UserRecord> = entries.iter().map(encode_row).collect();
        UserDocument {
            format: 1,
            sections: UserSections {
                journal: Some(rows),
                settings: None,
                proofs: None,
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        }
    }

    /// An engine stand-in over a reflection home.
    fn engine_swap(reflection: ReflectionState) -> StdArc<ArcSwap<AppState>> {
        StdArc::new(ArcSwap::from_pointee(AppState {
            reflection,
            ..AppState::default()
        }))
    }

    /// Applies the domain fold to the stand-in the way the engine arm
    /// does (the in-memory half of persist-then-interpret).
    fn apply_report(swap: &StdArc<ArcSwap<AppState>>, report: &UserDataReport) {
        let mut home = swap.load().reflection.clone();
        let _outcome = taskboard_domain::import_reflections(&mut home, &report.journal);
        swap.store(StdArc::new(AppState {
            reflection: home,
            ..AppState::default()
        }));
    }

    /// X1 — fold-first attach (the pinned ordering, D-ES5): with a peer
    /// journal at first contact and the lane deliberately full, the
    /// attach **blocks inside the import step** — the own appdata
    /// create-PUT cannot be issued. Draining one report releases it.
    /// A device that published before importing could resurrect its own
    /// tombstones over peers' deletes; the ordering is the pin.
    #[tokio::test(start_paused = true)]
    async fn x1_attach_imports_before_it_publishes() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        fake.seed_file(
            &appdata_path(PEER),
            doc_bytes(&journal_doc(&[entry(
                0x1000,
                "the peer's review",
                "2026-11-05T10:00:00Z".parse().unwrap(),
            )])),
        );
        // The full lane: capacity 4, pre-filled — the attach's send
        // suspends inside `import_step`.
        let (report_tx, report_rx) = mpsc::channel(4);
        for _ in 0..4 {
            report_tx
                .send(UserDataReport {
                    journal: Vec::new(),
                    settings: Vec::new(),
                    proofs: Vec::new(),
                })
                .await
                .expect("pre-fill");
        }
        let swap = engine_swap(ReflectionState::default());
        let local = StdArc::new(crate::cycle::PublishedState::new(swap.clone()));
        let harness = spawn_actor_with_reports(fake, local, (report_tx, report_rx));

        // Let the attach run up to the blocked send: no publish yet.
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(
            puts(&harness.fake, &appdata_path(SELF)) == 0,
            "fold-first: the import blocks the first publish; log: {:?}",
            log(&harness.fake)
        );
        // Release slots until the real delta crosses (the pre-fill's
        // four empty reports come off the queue first — FIFO).
        let mut report;
        loop {
            report = next_report(&harness).await;
            if !report.journal.is_empty() {
                break;
            }
        }
        assert_eq!(report.journal.len(), 1);
        let taskboard_domain::ImportedReflection::Entry(imported) = &report.journal[0] else {
            panic!("a live peer row imports as an entry");
        };
        assert_eq!(imported.text, "the peer's review");
        let status = next_status(&harness).await;
        assert!(ok(&status), "attach completes: {status:?}");
        assert_eq!(
            puts(&harness.fake, &appdata_path(SELF)),
            1,
            "the first publish follows the import"
        );

        // The steady state: the stand-in applies the report; the next
        // cycle is fully quiet — no report, no PUT.
        apply_report(&swap, &report);
        let (puts_before, status_before) = (
            puts(&harness.fake, &appdata_path(SELF)),
            harness_reports_pending(&harness),
        );
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert_eq!(puts(&harness.fake, &appdata_path(SELF)), puts_before);
        assert_eq!(harness_reports_pending(&harness), status_before);
    }

    /// Whether a report is pending on the stand-in's lane.
    fn harness_reports_pending(harness: &Harness) -> bool {
        harness.reports.borrow_mut().try_recv().is_ok()
    }

    /// X2 — a peer tombstone for an id the home never held: reported
    /// once; later cycles stay silent (the fingerprint dedupes the
    /// forever-different delta; the engine is independently no-op-safe).
    #[tokio::test(start_paused = true)]
    async fn x2_absent_target_tombstone_is_reported_exactly_once() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        let mut dead = entry(
            0x1001,
            "deleted before we ever saw it",
            "2026-11-05T11:00:00Z".parse().unwrap(),
        );
        dead.deleted = true;
        fake.seed_file(&appdata_path(PEER), doc_bytes(&journal_doc(&[dead])));
        let swap = engine_swap(ReflectionState::default());
        let local = StdArc::new(crate::cycle::PublishedState::new(swap.clone()));
        let harness = spawn_actor_with(fake, local);
        let report = next_report(&harness).await;
        assert!(
            matches!(
                report.journal.as_slice(),
                [taskboard_domain::ImportedReflection::Tombstone { .. }]
            ),
            "the tombstone crosses as a tombstone"
        );
        let status = next_status(&harness).await;
        assert!(ok(&status));
        // The engine skips the absent-target tombstone (no ghost rows);
        // the stand-in applies nothing. Later cycles: no report, no PUT.
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert!(
            !harness_reports_pending(&harness),
            "the fingerprint dedupes the forever-different delta"
        );
    }

    /// X3 — a peer row with a bad body (a newer build's out-of-range
    /// rating) is not reported, is counted on the cycle outcome, and is
    /// still relayed in the published document.
    #[tokio::test(start_paused = true)]
    async fn x3_bad_body_row_is_counted_but_still_relayed() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "unused");
        let (key, mut record) = encode_row(&entry(
            0x1002,
            "unreadable to this build",
            "2026-11-05T12:00:00Z".parse().unwrap(),
        ));
        let taskboard_sync_files::document::UserRecord::Entry { body, .. } = &mut record else {
            panic!("live shape");
        };
        body.insert("attributes".to_owned(), serde_json::json!({ "mood": 6 }));
        let mut rows = BTreeMap::new();
        rows.insert(key, record);
        let doc = UserDocument {
            format: 1,
            sections: UserSections {
                journal: Some(rows),
                settings: None,
                proofs: None,
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        };
        fake.seed_file(&appdata_path(PEER), doc_bytes(&doc));
        let harness = spawn_actor_with(fake, Arc::new(crate::cycle::EmptyDocument));
        let status = next_status(&harness).await;
        assert!(ok(&status));
        assert_eq!(
            status.last_cycle,
            Some(CycleOutcome::Ok {
                published: true,
                read_devices: 1,
                import_rejected: 1,
            }),
            "the bad body is counted, not swallowed: {status:?}"
        );
        assert!(
            !harness_reports_pending(&harness),
            "an undecodable row never crosses the seam"
        );
        let published = harness.fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).unwrap();
        assert!(
            document
                .sections
                .journal
                .expect("the section relays")
                .contains_key(&key_of(0x1002)),
            "the row still relays in the merged view"
        );
    }

    /// X3b — the rejection count is a standing finding, not a one-cycle
    /// event: while the undecodable row persists, every later cycle
    /// (deduped by the fingerprint — nothing re-crosses the seam) still
    /// carries the count.
    #[tokio::test(start_paused = true)]
    async fn x3b_the_rejection_count_persists_across_deduped_cycles() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "unused");
        let (key, mut record) = encode_row(&entry(
            0x1005,
            "still unreadable",
            "2026-11-05T12:00:00Z".parse().unwrap(),
        ));
        let taskboard_sync_files::document::UserRecord::Entry { body, .. } = &mut record else {
            panic!("live shape");
        };
        body.insert("attributes".to_owned(), serde_json::json!({ "mood": 6 }));
        let doc = UserDocument {
            format: 1,
            sections: UserSections {
                journal: Some(BTreeMap::from([(key, record)])),
                settings: None,
                proofs: None,
                unknown: BTreeMap::new(),
            },
            unknown: BTreeMap::new(),
        };
        fake.seed_file(&appdata_path(PEER), doc_bytes(&doc));
        let harness = spawn_actor_with(fake, Arc::new(crate::cycle::EmptyDocument));
        let status = next_status(&harness).await;
        assert!(ok(&status));
        for _ in 0..2 {
            let status = next_cycle(&harness).await;
            assert!(
                matches!(
                    status.last_cycle,
                    Some(CycleOutcome::Ok {
                        import_rejected: 1,
                        ..
                    })
                ),
                "the count stands while the row does: {status:?}"
            );
            assert!(!harness_reports_pending(&harness));
        }
    }

    /// The UUID-text key the fixture entries encode under.
    fn key_of(id_raw: u128) -> String {
        uuid::Uuid::from_u128(id_raw).to_string()
    }

    /// X4 — the state-derived local document (D-ES8): a live entry
    /// encodes as the entity's wire body (no `id`/`updated_at`/`deleted`
    /// out of band); a tombstoned entry emits the wire tombstone —
    /// silent absence would let a peer resurrect a deleted entry.
    #[test]
    fn x4_published_journal_materialises_tombstones_and_entries() {
        let mut home = ReflectionState::default();
        let live = entry(
            0x1003,
            "still here",
            "2026-11-05T13:00:00Z".parse().unwrap(),
        );
        let mut dead = entry(0x1004, "gone", "2026-11-05T14:00:00Z".parse().unwrap());
        dead.deleted = true;
        home.entries.insert(live.id, live.clone());
        home.entries.insert(dead.id, dead.clone());
        let document = crate::cycle::PublishedState::new(engine_swap(home)).document();
        assert_eq!(
            document.format,
            taskboard_sync_files::document::DOCUMENT_FORMAT
        );
        let rows = document
            .sections
            .journal
            .expect("a non-empty home publishes the section");
        assert_eq!(rows.len(), 2, "tombstones included");
        let (key, record) = encode_row(&live);
        assert_eq!(rows[&key], record);
        let (dead_key, dead_record) = encode_row(&dead);
        assert_eq!(rows[&dead_key], dead_record);
        assert!(matches!(rows[&dead_key], UserRecord::Tombstone { .. }));
    }

    // -- chunk 4's findings: peer freshness + the divergence window ----

    fn peer_id() -> DeviceId {
        DeviceId::parse(PEER).expect("fixture peer id")
    }

    /// A heartbeat fixture with a chosen `state_hash` (the type decodes
    /// any tagged string, so a disagreement is just a different one).
    fn heartbeat_bytes(state_hash: &str) -> Vec<u8> {
        serde_json::json!({
            "app_version": "test",
            "format_version": 1,
            "last_clock": "2026-11-05T09:00:00Z",
            "state_hash": state_hash,
            "written_at": "2026-11-05T09:30:00Z"
        })
        .to_string()
        .into_bytes()
    }

    /// The view hash this fleet would converge on for the given peer
    /// text: read off the *published* own appdata (the honest source —
    /// the fold's output, not a fixture-side recomputation).
    fn published_view_hash(fake: &FakeFilesTransport) -> String {
        let published = fake.file(&appdata_path(SELF)).expect("published");
        let document: UserDocument = serde_json::from_slice(&published).expect("decodes");
        StateHash::of(&document).to_string()
    }

    /// 16 — a peer heartbeat lands in the watch: `status.peers` maps
    /// the decoded `info.json` facts.
    #[tokio::test(start_paused = true)]
    async fn matrix_16_peer_heartbeat_lands_in_the_watch() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        fake.seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:whatever"));
        let harness = spawn_actor(fake);
        let status = next_status(&harness).await;
        assert!(ok(&status), "{status:?}");
        let freshness = status.peers.get(&peer_id()).expect("peer freshness");
        assert_eq!(
            freshness,
            &PeerFreshness {
                written_at: "2026-11-05T09:30:00Z".parse().expect("clock"),
                last_clock: "2026-11-05T09:00:00Z".parse().expect("clock"),
            }
        );
    }

    /// 17 — divergence never fires while writes pend: a changing peer
    /// keeps the view moving (published every cycle ⇒ busy), so the
    /// disagreeing heartbeat can never close the window.
    #[tokio::test(start_paused = true)]
    async fn matrix_17_divergence_never_fires_while_writes_pend() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        fake.seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:disagreeing"));
        let harness = spawn_actor(fake);
        for round in 0..3 {
            harness
                .fake
                .seed_file(&appdata_path(PEER), doc_bytes(&doc(&format!("v{round}"))));
            let status = next_cycle(&harness).await;
            assert!(ok(&status), "{status:?}");
            assert!(
                status.diverged_from.is_empty(),
                "busy cycle must reset, not fire: {status:?}"
            );
            assert!(status.peers.contains_key(&peer_id()));
        }
    }

    /// 18 — the window fires: a disagreeing heartbeat observed on the
    /// first quiet cycle does not fire; the second consecutive quiet
    /// cycle does.
    #[tokio::test(start_paused = true)]
    async fn matrix_18_the_quiescence_window_is_two_cycles() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake);
        let status = next_status(&harness).await;
        assert!(ok(&status));
        // Rewrite only the heartbeat: disagreeing hash, content same.
        harness.fake.stage(Effect::Pass);
        harness
            .fake
            .seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:disagreeing"));
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert!(
            status.diverged_from.is_empty(),
            "the first quiet cycle must not fire: {status:?}"
        );
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert_eq!(
            status.diverged_from,
            std::collections::BTreeSet::from([peer_id()]),
            "the second quiet cycle closes the window: {status:?}"
        );
    }

    /// 23 — a merely-slow peer is never divergent: the peer converged
    /// on our view, then went offline; a local edit moves our view past
    /// its frozen heartbeat. The peer has not absorbed our publish, so
    /// the disagreement is lag (its heartbeat age says so), not
    /// divergence — however many quiet cycles pass.
    #[tokio::test(start_paused = true)]
    async fn matrix_23_an_offline_peer_never_reads_as_diverged() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        let peer_row = entry(
            0x2001,
            "the peer's",
            "2026-11-05T10:00:00Z".parse().unwrap(),
        );
        fake.seed_file(&appdata_path(PEER), doc_bytes(&journal_doc(&[peer_row])));
        let swap = engine_swap(ReflectionState::default());
        let local = StdArc::new(crate::cycle::PublishedState::new(swap.clone()));
        let harness = spawn_actor_with(fake, local);
        apply_report(&swap, &next_report(&harness).await);
        assert!(ok(&next_status(&harness).await));
        // The peer converges on our view and says so.
        let converged = published_view_hash(&harness.fake);
        harness
            .fake
            .seed_file(&info_path(PEER), heartbeat_bytes(&converged));
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert!(status.diverged_from.is_empty(), "{status:?}");
        // The peer goes offline; this device edits locally.
        let mut home = swap.load().reflection.clone();
        let own = entry(
            0x2002,
            "ours, later",
            "2026-11-05T11:00:00Z".parse().unwrap(),
        );
        home.entries.insert(own.id, own);
        swap.store(StdArc::new(AppState {
            reflection: home,
            ..AppState::default()
        }));
        for _ in 0..4 {
            let status = next_cycle(&harness).await;
            assert!(ok(&status));
            assert!(
                status.diverged_from.is_empty(),
                "a lagging peer is an age, not a divergence: {status:?}"
            );
        }
    }

    /// 19 — agreement clears: a restored matching heartbeat empties
    /// the finding on the next quiet cycle.
    #[tokio::test(start_paused = true)]
    async fn matrix_19_agreement_clears_the_finding() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake);
        let status = next_status(&harness).await;
        assert!(ok(&status));
        harness
            .fake
            .seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:disagreeing"));
        let _ = next_cycle(&harness).await;
        let status = next_cycle(&harness).await;
        assert_eq!(
            status.diverged_from,
            std::collections::BTreeSet::from([peer_id()])
        );
        let matching = published_view_hash(&harness.fake);
        harness.fake.stage(Effect::Pass);
        harness
            .fake
            .seed_file(&info_path(PEER), heartbeat_bytes(&matching));
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert!(
            status.diverged_from.is_empty(),
            "agreement is evidence too: {status:?}"
        );
    }

    /// 20 — parked exclusion: a garbage peer document quarantines the
    /// park, and the (healthy, disagreeing) heartbeat can never fire
    /// divergence for a peer this device cannot fold.
    #[tokio::test(start_paused = true)]
    async fn matrix_20_parked_peer_is_never_diverged() {
        let fake = FakeFilesTransport::new();
        let dirs = dirs_of(&[SELF, PEER]);
        let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
        fake.seed_dirs(&refs);
        fake.seed_file(&appdata_path(PEER), b"{{{ not json".to_vec());
        fake.seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:disagreeing"));
        let harness = spawn_actor(fake);
        let status = next_status(&harness).await;
        assert!(ok(&status));
        assert_eq!(
            status.parked.get(&appdata_path(PEER)),
            Some(&ParkKind::Quarantined)
        );
        assert!(status.diverged_from.is_empty());
    }

    /// 21 — vanish: after the finding is active, the peer subtree's
    /// removal drops it from `peers` and `diverged_from` on the next
    /// Ok cycle.
    #[tokio::test(start_paused = true)]
    async fn matrix_21_vanished_peer_drops_from_freshness_and_divergence() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        fake.seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:disagreeing"));
        let harness = spawn_actor(fake);
        let _ = next_status(&harness).await;
        // Attach was busy; two quiet cycles close the window.
        let _ = next_cycle(&harness).await;
        let status = next_cycle(&harness).await;
        assert!(status.diverged_from.contains(&peer_id()));
        harness
            .fake
            .remove_device_subtree(&format!("taskboard/devices/{PEER}"));
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert!(!status.peers.contains_key(&peer_id()), "{status:?}");
        assert!(status.diverged_from.is_empty(), "{status:?}");
    }

    /// 22 — the Failed row of the rule table: a transport failure
    /// between the two quiet cycles holds the streak (it neither
    /// advances nor resets), so the window still closes on the next
    /// quiet cycle.
    #[tokio::test(start_paused = true)]
    async fn matrix_22_a_failed_cycle_holds_the_window() {
        let fake = FakeFilesTransport::new();
        seed_peer(&fake, "v1");
        let harness = spawn_actor(fake);
        let status = next_status(&harness).await;
        assert!(ok(&status));
        // Quiet cycle 1: the disagreeing heartbeat is observed.
        harness.fake.stage(Effect::Pass);
        harness
            .fake
            .seed_file(&info_path(PEER), heartbeat_bytes("fnv1a64:disagreeing"));
        let _ = next_cycle(&harness).await;
        // A failing cycle: the PROPFIND (the cycle's first request)
        // answers 503 — no evidence, the streak must hold at 1.
        harness.fake.stage(Effect::Status(503));
        let status = next_cycle(&harness).await;
        assert_eq!(status.last_cycle, Some(CycleOutcome::Failed));
        // Quiet cycle 2 (the peer is still dirty): the window closes.
        let status = next_cycle(&harness).await;
        assert!(ok(&status));
        assert_eq!(
            status.diverged_from,
            std::collections::BTreeSet::from([peer_id()]),
            "the failure held the streak, it did not reset it: {status:?}"
        );
    }

    /// The board lanes' empty state seam: no boards, no outbox — every
    /// existing per-user matrix test runs lane-free.
    #[derive(Debug)]
    struct EmptyPersistedState;

    impl taskboard_domain::persistence::SyncStateReader for EmptyPersistedState {
        fn read_state(
            &self,
        ) -> taskboard_domain::BoxFuture<
            '_,
            Result<taskboard_domain::PersistedState, taskboard_domain::RepositoryError>,
        > {
            Box::pin(async { Ok(taskboard_domain::PersistedState::default()) })
        }
        fn read_settings(&self) -> taskboard_domain::SettingsSnapshot {
            taskboard_domain::SettingsSnapshot::default()
        }
    }
}

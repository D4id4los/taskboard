// SPDX-License-Identifier: MIT OR Apache-2.0
//! The retained-ETag characterization oracle (chunk E, #695 + #133).
//!
//! A stateful wiremock backend models the *hosted* server behavior the
//! #695 curl evidence captured: a same-second, same-byte-length guarded
//! PUT returns `204` and **keeps the old `ETag`** (retention); a different
//! length or a later second churns it; a genuinely stale `If-Match`
//! still answers `412` — retention never rescues a stale token. All
//! time is injected: the model's write second is a test-set value, the
//! cycle's stamps ride a stepping clock — no wall clock, no sleeps
//! (standing law b).
//!
//! Scenario naming (the stage-1 plan §6): S1 false-clean pull, S2
//! lost-update guard, S3 stale-token bounded convergence, S4 the
//! non-pathological twins. Assertions are typed outcomes
//! (`forked_paths`, `CycleVerdict`) plus the model's request journal
//! (method/path counts) — never status or log text (standing law a).
//!
//! Commit-order discipline: this file is the oracle — S1–S3 are RED
//! against the pre-fix client (that is the characterization), S4 is
//! GREEN both before and after (the corpus-unchanged guarantee at the
//! wire level).

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use taskboard_domain::Clock;
use taskboard_domain::ids::ConnectorId;
use taskboard_domain::messages::{SyncReport, UserDataReport};
use taskboard_domain::persistence::{PersistedState, RepositoryError, SyncStateReader};
use taskboard_domain::settings::SettingsSnapshot;
use taskboard_sync_files::document::{DOCUMENT_FORMAT, UserDocument, UserRecord, UserSections};
use taskboard_sync_files::hash::StateHash;
use taskboard_sync_files::heartbeat::DeviceInfo;
use taskboard_sync_files::identity::DeviceIdentity;
use taskboard_sync_files::ids::{AuthorId, DeviceId};
use taskboard_sync_files_nextcloud::client::WebDavFilesTransport;
use taskboard_sync_files_nextcloud::cycle::{
    CycleVerdict, LocalDocument, LoopState, SyncContext, run_cycle,
};
use tokio::sync::{mpsc, watch};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const USER: &str = "alice";
const DAV_PREFIX: &str = "/remote.php/dav/files/alice/";
const SELF: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a";
const PEER: &str = "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5b";
const ROW_KEY: &str = "0193c000-0000-7c89-ab01-2345abcdef01";
/// The model second all attach-time writes land on; retention windows
/// are expressed relative to it.
const ATTACH_SECOND: i64 = 1_000;

// ------------------------------------------------------------------
// The modeled server.
// ------------------------------------------------------------------

/// One stored file: bytes, revision (the etag's only ingredient), and
/// the model second of its last write (the retention window).
struct Stored {
    body: Vec<u8>,
    rev: u64,
    last_write_second: i64,
}

/// One journaled request (the tests' exact observable).
#[derive(Debug)]
struct Recorded {
    method: String,
    path: String,
}

#[derive(Default)]
struct Model {
    /// The model's current second — test-set, never a wall read.
    second: i64,
    dirs: BTreeMap<String, u64>,
    files: BTreeMap<String, Stored>,
    journal: Vec<Recorded>,
    /// Queued one-shot forced 412s for a path's next guarded PUTs (the
    /// give-up twin's second rejection; a set cannot stage two).
    forced_412: VecDeque<String>,
    /// One-shot interleaving directive: after the next PUT to `path`,
    /// another writer's PUT of `body` lands through the retention rule
    /// before any further request is answered — the real fleet's
    /// same-second race (the sibling's request arriving between this
    /// client's PUT and its verify GET).
    inject_after_put: Option<(String, Vec<u8>)>,
}

impl Model {
    fn etag_of(rev: u64) -> String {
        format!(r#""rev{rev}""#)
    }

    fn bump_ancestors(&mut self, path: &str) {
        let mut ancestors = path.split('/').collect::<Vec<_>>();
        ancestors.pop(); // the file itself is not a collection
        let mut prefix = String::new();
        for component in ancestors {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            self.dirs.entry(prefix.clone()).and_modify(|rev| *rev += 1);
        }
    }

    /// The PUT rule verbatim from the stage-1 plan §6: create-once
    /// honours `If-None-Match: *`; a guarded PUT whose token is not the
    /// current etag answers 412 (retention never rescues a stale
    /// token); a matching PUT at the same model second with the same
    /// byte length **retains** the etag (the #695 hosted signature);
    /// anything else churns it.
    fn put(
        &mut self,
        path: &str,
        body: &[u8],
        if_match: Option<&str>,
        if_none_match_star: bool,
    ) -> (u16, Option<String>) {
        if if_none_match_star {
            if self.files.contains_key(path) {
                return (412, None);
            }
            self.files.insert(
                path.to_owned(),
                Stored {
                    body: body.to_vec(),
                    rev: 1,
                    last_write_second: self.second,
                },
            );
            self.bump_ancestors(path);
            return (201, Some(Self::etag_of(1)));
        }
        if let Some(pos) = self.forced_412.iter().position(|p| p == path) {
            self.forced_412.remove(pos);
            return (412, None);
        }
        let Some(stored) = self.files.get_mut(path) else {
            return (404, None);
        };
        if if_match != Some(Self::etag_of(stored.rev).as_str()) {
            return (412, None);
        }
        let retained = self.second == stored.last_write_second && body.len() == stored.body.len();
        if retained {
            // The hosted signature: content lands, the etag stays — and
            // so does every ancestor collection etag.
            stored.body = body.to_vec();
            stored.last_write_second = self.second;
            let etag = Self::etag_of(stored.rev);
            return (204, Some(etag));
        }
        stored.rev += 1;
        stored.body = body.to_vec();
        stored.last_write_second = self.second;
        let etag = Self::etag_of(stored.rev);
        self.bump_ancestors(path);
        (204, Some(etag))
    }

    /// Fires the staged interleaving directive after a PUT to `path`.
    fn maybe_inject(&mut self, path: &str) {
        if let Some((inject_path, body)) = self.inject_after_put.take()
            && inject_path == path
        {
            self.peer_write(&inject_path, &body);
        }
    }

    /// A peer writer's unguarded PUT through the same retention rule
    /// (another clone of this device writes its own path).
    fn peer_write(&mut self, path: &str, body: &[u8]) {
        let current = self.files.get(path).map(|s| Self::etag_of(s.rev));
        let _ = self.put(path, body, current.as_deref(), false);
    }

    /// The Depth-1 listing body: child collections of `prefix`, one
    /// `response` member each, quoted etags verbatim.
    fn listing_xml(&self, prefix: &str) -> String {
        let mut body = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <d:multistatus xmlns:d=\"DAV:\">",
        );
        let dir_prefix = format!("{prefix}/");
        for (path, rev) in &self.dirs {
            let Some(rest) = path.strip_prefix(&dir_prefix) else {
                continue;
            };
            if rest.is_empty() || rest.contains('/') {
                continue;
            }
            let _ = write!(
                body,
                "<d:response><d:href>{DAV_PREFIX}{path}/</d:href>\
                 <d:propstat><d:prop><d:getetag>{}</d:getetag></d:prop>\
                 <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
                Self::etag_of(*rev),
            );
        }
        body.push_str("</d:multistatus>");
        body
    }
}

/// The shared handle the tests drive (seed, clock, journal) and the
/// `Respond` impl serves from.
#[derive(Clone)]
struct RetentionServer {
    model: Arc<Mutex<Model>>,
}

impl RetentionServer {
    fn new() -> Self {
        Self {
            model: Arc::new(Mutex::new(Model::default())),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Model> {
        self.model.lock().expect("model lock")
    }

    fn set_second(&self, second: i64) {
        self.lock().second = second;
    }

    fn seed_dir(&self, path: &str) {
        self.lock().dirs.insert(path.to_owned(), 0);
    }

    fn seed_file(&self, path: &str, body: Vec<u8>) {
        let mut model = self.lock();
        let second = model.second;
        model.files.insert(
            path.to_owned(),
            Stored {
                body,
                rev: 1,
                last_write_second: second,
            },
        );
        model.bump_ancestors(path);
    }

    /// Another writer's unguarded PUT through the retention rule.
    fn peer_write(&self, path: &str, body: &[u8]) {
        self.lock().peer_write(path, body);
    }

    /// Stages one forced 412 for the path's next guarded PUT (model
    /// unchanged).
    fn force_412(&self, path: &str) {
        self.lock().forced_412.push_back(path.to_owned());
    }

    /// Stages the same-second interleaving: after the path's next PUT,
    /// another writer's PUT of `body` lands before the next request is
    /// answered (the lost-update race, S2).
    fn inject_write_after_next_put(&self, path: &str, body: Vec<u8>) {
        self.lock().inject_after_put = Some((path.to_owned(), body));
    }

    fn count(&self, method: &str, path: &str) -> usize {
        self.lock()
            .journal
            .iter()
            .filter(|req| req.method == method && req.path == path)
            .count()
    }

    fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.lock().files.get(path).map(|s| s.body.clone())
    }
}

impl Respond for RetentionServer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut model = self.lock();
        let raw_path = request.url.path();
        let Some(path) = raw_path.strip_prefix(DAV_PREFIX) else {
            return ResponseTemplate::new(404);
        };
        model.journal.push(Recorded {
            method: request.method.as_str().to_owned(),
            path: path.to_owned(),
        });
        match request.method.as_str() {
            "MKCOL" => ResponseTemplate::new(201),
            "GET" => match model.files.get(path) {
                Some(stored) => ResponseTemplate::new(200)
                    .set_body_bytes(stored.body.clone())
                    .insert_header("ETag", Model::etag_of(stored.rev).as_str()),
                None => ResponseTemplate::new(404),
            },
            "PUT" => {
                let if_match = request
                    .headers
                    .get("if-match")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let if_none_match_star = request
                    .headers
                    .get("if-none-match")
                    .and_then(|v| v.to_str().ok())
                    == Some("*");
                let (status, etag) =
                    model.put(path, &request.body, if_match.as_deref(), if_none_match_star);
                model.maybe_inject(path);
                let mut response = ResponseTemplate::new(status);
                if let Some(etag) = etag {
                    response = response.insert_header("ETag", etag.as_str());
                }
                response
            }
            "PROPFIND" => {
                let prefix = path.trim_end_matches('/');
                ResponseTemplate::new(207).set_body_string(model.listing_xml(prefix))
            }
            _ => ResponseTemplate::new(405),
        }
    }
}

// ------------------------------------------------------------------
// The cycle harness: `run_cycle` driven directly, all clocks injected.
// ------------------------------------------------------------------

/// A logical stepping clock — 30 s per read from a fixed base. No wall
/// read anywhere (standing law b).
#[derive(Debug)]
struct TestClock {
    next_secs: AtomicI64,
}

impl TestClock {
    fn new(start_secs: i64) -> Self {
        Self {
            next_secs: AtomicI64::new(start_secs),
        }
    }
}

impl Clock for TestClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone as _;
        let secs = self.next_secs.fetch_add(30, Ordering::Relaxed);
        chrono::Utc
            .timestamp_opt(secs, 0)
            .single()
            .expect("in range")
    }
}

/// A swappable local contribution: the test sets the document each
/// cycle's publish should fold.
#[derive(Debug)]
struct SwappableLocal(Mutex<UserDocument>);

impl LocalDocument for SwappableLocal {
    fn document(&self) -> UserDocument {
        self.0.lock().expect("local lock").clone()
    }
}

/// The engine stand-in: an empty persisted state (no files-owned
/// boards — the board lanes' scan sees an empty listing).
#[derive(Debug)]
struct NoBoards;

impl SyncStateReader for NoBoards {
    fn read_state(
        &self,
    ) -> taskboard_domain::persistence::BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(async { Ok(PersistedState::default()) })
    }

    fn read_settings(&self) -> SettingsSnapshot {
        SettingsSnapshot::default()
    }
}

fn doc(text: &str) -> UserDocument {
    doc_with_clock(text, "2026-11-05T08:00:00Z")
}

/// `doc` with the row clock spelled: the S3 scenarios give the
/// sibling's write an *older* clock so the re-folded view (this
/// device's newer row wins the LWW fold, no ADR-0004 tie) still
/// differs from served — a genuine convergence attempt, not a tie.
fn doc_with_clock(text: &str, clock: &str) -> UserDocument {
    let mut body = BTreeMap::new();
    body.insert("text".to_owned(), serde_json::json!(text));
    let mut journal = BTreeMap::new();
    journal.insert(
        ROW_KEY.to_owned(),
        UserRecord::Entry {
            body,
            clock: clock.parse().expect("clock"),
            author: None::<AuthorId>,
        },
    );
    UserDocument {
        format: DOCUMENT_FORMAT,
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

/// A peer heartbeat fixture with the given `written_at` (fixed-width
/// RFC3339, so same-second rewrites are always same-byte-length — the
/// #695 premise).
fn heartbeat_bytes(written_at: chrono::DateTime<chrono::Utc>, state_hash: &StateHash) -> Vec<u8> {
    serde_json::to_vec(&DeviceInfo {
        app_version: "test".to_owned(),
        format_version: 1,
        last_clock: "2026-11-05T08:00:00Z".parse().expect("clock"),
        state_hash: state_hash.clone(),
        written_at,
    })
    .expect("heartbeat serializes")
}

fn appdata_path(id: &str) -> String {
    format!("taskboard/devices/{id}/appdata.json")
}

fn info_path(id: &str) -> String {
    format!("taskboard/devices/{id}/info.json")
}

struct Harness {
    server: RetentionServer,
    transport: WebDavFilesTransport,
    // The mounted server's ownership anchor (#1165–#1170): the
    // `MockServer` handle must live as long as the harness — dropped
    // at constructor end it starts wiremock's async shutdown
    // mid-test, the listener closes, and the OS hands the ephemeral
    // port to a concurrently starting suite member's server; the
    // test's remaining requests then land on the *other* test's
    // stateful model (second attach 412, foreign journals — the
    // flaky class). Holding the handle is the structural fix: the
    // server cannot die before the test does.
    _mounted: MockServer,
    ctx: SyncContext,
    st: LoopState,
    status_tx: watch::Sender<taskboard_sync_files_nextcloud::status::FilesSyncStatus>,
    status_rx: watch::Receiver<taskboard_sync_files_nextcloud::status::FilesSyncStatus>,
    local: Arc<SwappableLocal>,
    _reports: mpsc::Receiver<UserDataReport>,
    _board_reports: mpsc::Receiver<SyncReport>,
}

impl Harness {
    /// A harness over `local` as this device's contribution. With
    /// `with_peer`, the peer subtree is seeded (appdata + heartbeat)
    /// at the attach second *before* the first cycle.
    async fn new(local: UserDocument, with_peer: bool) -> Self {
        let server = RetentionServer::new();
        // All seed writes land on the attach second, so the scenarios'
        // same-second rewrites hit the retention window.
        server.set_second(ATTACH_SECOND);
        let mounted = MockServer::start().await;
        Mock::given(any())
            .respond_with(server.clone())
            .mount(&mounted)
            .await;
        let transport =
            WebDavFilesTransport::new(&mounted.uri(), USER, "t").expect("server URL is valid");
        server.seed_dir("taskboard");
        server.seed_dir("taskboard/devices");
        server.seed_dir(&format!("taskboard/devices/{SELF}"));
        if with_peer {
            server.seed_dir(&format!("taskboard/devices/{PEER}"));
            server.seed_file(&appdata_path(PEER), doc_bytes(&doc("p1")));
            let written_at = "2026-01-01T00:00:00Z".parse().expect("clock");
            server.seed_file(
                &info_path(PEER),
                heartbeat_bytes(written_at, &StateHash::of(&doc("p1"))),
            );
        }
        let local = Arc::new(SwappableLocal(Mutex::new(local)));
        let (report_tx, report_rx) = mpsc::channel(16);
        let (board_tx, board_rx) = mpsc::channel::<SyncReport>(16);
        let (status_tx, status_rx) =
            watch::channel(taskboard_sync_files_nextcloud::status::FilesSyncStatus::default());
        let identity = DeviceIdentity {
            device_id: DeviceId::parse(SELF).expect("fixture id"),
            created_at: "2026-11-04T00:00:00Z".parse().expect("clock"),
        };
        let ctx = SyncContext {
            identity,
            app_version: "test".to_owned(),
            clock: Arc::new(TestClock::new(1_760_000_000)),
            local: local.clone(),
            report_tx,
            connector: ConnectorId::from("files-sync"),
            state_reader: Arc::new(NoBoards),
            board_reports: board_tx,
            deck_origins: BTreeMap::new(),
            cancel: taskboard_sync_files_nextcloud::cycle::CycleCancel::new(),
        };
        Self {
            server,
            transport,
            _mounted: mounted,
            ctx,
            st: LoopState::default(),
            status_tx,
            status_rx,
            local,
            _reports: report_rx,
            _board_reports: board_rx,
        }
    }

    async fn cycle(&mut self) -> CycleVerdict {
        run_cycle(&self.transport, &self.ctx, &mut self.st, &self.status_tx).await
    }

    fn set_local(&self, document: UserDocument) {
        *self.local.0.lock().expect("local lock") = document;
    }

    fn puts(&self, path: &str) -> usize {
        self.server.count("PUT", path)
    }

    fn gets(&self, path: &str) -> usize {
        self.server.count("GET", path)
    }
}

fn forked(harness: &Harness, path: &str) -> bool {
    harness.status_rx.borrow().forked_paths.contains(path)
}

// ------------------------------------------------------------------
// S1 — the false-clean pull (the #695 bug, red before the fix).
// ------------------------------------------------------------------

/// A peer rewrites its appdata and heartbeat in the same model second
/// with the same byte length: every validator is retained, yet the
/// peer heartbeat's `written_at` (the revision field) advanced far
/// past the skew band. The next cycle must pull the peer (its appdata
/// re-read) — before the row-6 fix this yields zero peer GETs.
#[tokio::test]
async fn s1_retained_etag_peer_rewrite_is_still_pulled_revision_field_precedence() {
    let mut harness = Harness::new(doc("v1"), true).await;
    harness.server.set_second(ATTACH_SECOND);
    let verdict = harness.cycle().await; // attach: the peer is read once
    let CycleVerdict::Completed { .. } = verdict else {
        panic!("attach must complete: {verdict:?}");
    };
    let gets_after_attach = harness.gets(&appdata_path(PEER));

    // The peer's same-second, same-size rewrite: both validators
    // retained, the heartbeat revision advances beyond the band.
    harness.server.set_second(ATTACH_SECOND);
    harness
        .server
        .peer_write(&appdata_path(PEER), &doc_bytes(&doc("p2")));
    let advanced = "2026-01-01T05:00:00Z".parse().expect("clock");
    harness.server.peer_write(
        &info_path(PEER),
        &heartbeat_bytes(advanced, &StateHash::of(&doc("p2"))),
    );
    assert_eq!(
        harness.gets(&appdata_path(PEER)),
        gets_after_attach,
        "precondition: the rewrite retained the validators"
    );

    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { read_devices, .. } = verdict else {
        panic!("steady cycle must complete: {verdict:?}");
    };
    assert_eq!(read_devices, 1, "the retained-etag peer is dirty");
    assert_eq!(
        harness.gets(&appdata_path(PEER)),
        gets_after_attach + 1,
        "revision-field precedence: the peer's appdata is re-read despite \
         the retained dir etag (red before the row-6 fix)"
    );
}

// ------------------------------------------------------------------
// S2 — the lost-update guard (retention signature ⇒ verify GET).
// ------------------------------------------------------------------

/// Both clones hold token E0. The sibling clone's same-second
/// same-size write lands first (the server retains E0); this clone's
/// guarded PUT therefore also "succeeds" with E0 — the retention
/// signature. The verify GET must detect the content mismatch and run
/// the typed `ForkDetected` path — no silent overwrite (red before
/// the row-7 fix).
#[tokio::test]
async fn s2_retention_signature_put_verifies_and_forks_on_mismatch() {
    // Attach publishes "aaaa" (create, etag E0, at ATTACH_SECOND).
    let mut harness = Harness::new(doc("aaaa"), false).await;
    harness.server.set_second(ATTACH_SECOND);
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { .. } = verdict else {
        panic!("attach must complete: {verdict:?}");
    };
    let path = appdata_path(SELF);
    assert_eq!(harness.puts(&path), 1);

    // The sibling clone's same-second same-size write lands *between*
    // this clone's PUT and its verify GET (the real fleet's race; the
    // staged directive answers the sibling's request there). Both
    // clones hold token E0: this clone's guarded PUT is answered 204
    // with E0 — indistinguishable from success without a GET.
    harness.server.set_second(ATTACH_SECOND);
    harness
        .server
        .inject_write_after_next_put(&path, doc_bytes(&doc("cccc")));
    harness.set_local(doc("bbbb"));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { published, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert!(published);
    assert!(
        forked(&harness, &path),
        "the retention signature's verify GET must detect the lost update \
         (red before the row-7 fix)"
    );
    assert_eq!(
        harness.puts(&path),
        2,
        "exactly one PUT this cycle: the signature write; no retry loop"
    );
    assert_eq!(harness.gets(&path), 1, "exactly the verify GET — bounded");
}

/// The converged twin: the served bytes equal the sent bytes, so the
/// verify GET converges — the token is refreshed, no fork.
#[tokio::test]
async fn s2t_converged_retention_signature_refreshes_token_without_fork() {
    let mut harness = Harness::new(doc("aaaa"), false).await;
    harness.server.set_second(ATTACH_SECOND);
    let _ = harness.cycle().await;
    let path = appdata_path(SELF);
    // The sibling wrote the *same* content we are about to publish.
    harness.server.set_second(ATTACH_SECOND);
    harness.server.peer_write(&path, &doc_bytes(&doc("bbbb")));
    harness.set_local(doc("bbbb"));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { published, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert!(published);
    assert!(!forked(&harness, &path), "identical content is not a fork");
    assert_eq!(harness.gets(&path), 1, "the verify GET ran");
}

// ------------------------------------------------------------------
// S3 — the stale-token 412 family: bounded convergence, typed give-up.
// ------------------------------------------------------------------

/// A stale token answers 412 (retention never rescues it); the client
/// re-resolves: one GET, re-fold, exactly one re-PUT with the
/// refreshed token — published, no fork, no loop.
#[tokio::test]
async fn s3_stale_token_412_converges_with_one_reresolution() {
    let mut harness = Harness::new(doc("aaaa"), false).await;
    harness.server.set_second(ATTACH_SECOND);
    let _ = harness.cycle().await;
    let path = appdata_path(SELF);

    // A sibling churns the etag (later second, different size): this
    // clone's stored token is now genuinely stale.
    harness.server.set_second(ATTACH_SECOND + 100);
    harness.server.peer_write(
        &path,
        &doc_bytes(&doc_with_clock("zzzzzzzz", "2026-11-05T07:00:00Z")),
    );
    let puts_before = harness.puts(&path);

    harness.set_local(doc("bbbb"));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { published, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert!(published);
    assert!(!forked(&harness, &path));
    assert_eq!(
        harness.puts(&path) - puts_before,
        2,
        "the 412'd attempt plus exactly one re-PUT — the retry bound"
    );
    assert_eq!(
        harness.gets(&path),
        1,
        "one disambiguation GET feeds the re-resolution"
    );
    // The converged view won the rewrite.
    let served = harness.server.file(&path).expect("published");
    let document: UserDocument = serde_json::from_slice(&served).expect("valid document");
    let row = document.sections.journal.expect("rows")[ROW_KEY].clone();
    assert!(
        matches!(row, UserRecord::Entry { ref body, .. }
            if body["text"] == serde_json::json!("bbbb")),
        "the re-resolution wrote this device's view"
    );
}

/// The give-up twin: the re-PUT 412s too — typed `ForkDetected`, and
/// no third PUT ever flies (request-count oracle).
#[tokio::test]
async fn s3t_second_412_gives_up_typed_no_third_put() {
    let mut harness = Harness::new(doc("aaaa"), false).await;
    harness.server.set_second(ATTACH_SECOND);
    let _ = harness.cycle().await;
    let path = appdata_path(SELF);

    harness.server.set_second(ATTACH_SECOND + 100);
    harness.server.peer_write(
        &path,
        &doc_bytes(&doc_with_clock("zzzzzzzz", "2026-11-05T07:00:00Z")),
    );
    harness.server.force_412(&path);
    harness.server.force_412(&path);
    let puts_before = harness.puts(&path);

    harness.set_local(doc("bbbb"));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { published, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert!(forked(&harness, &path), "the second 412 is a typed fork");
    assert_eq!(
        harness.puts(&path) - puts_before,
        2,
        "the initial attempt and the single re-PUT — never a third"
    );
    let _ = published;
}

// ------------------------------------------------------------------
// S4 — the non-pathological twins: today's behavior, request counts.
// ------------------------------------------------------------------

/// A peer rewrite in a *later* second churns the dir etag: the full
/// dirty read runs exactly as today — appdata + info, and nothing
/// else (no retention machinery on the healthy path).
#[tokio::test]
async fn s4a_churned_dir_etag_full_read_is_unchanged() {
    let mut harness = Harness::new(doc("v1"), true).await;
    harness.server.set_second(ATTACH_SECOND);
    let _ = harness.cycle().await;
    let (appdata_gets, info_gets) = (
        harness.gets(&appdata_path(PEER)),
        harness.gets(&info_path(PEER)),
    );

    harness.server.set_second(ATTACH_SECOND + 100);
    harness
        .server
        .peer_write(&appdata_path(PEER), &doc_bytes(&doc("p2")));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { read_devices, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert_eq!(read_devices, 1);
    assert_eq!(
        harness.gets(&appdata_path(PEER)),
        appdata_gets + 1,
        "the dirty read, exactly as today"
    );
    assert_eq!(
        harness.gets(&info_path(PEER)),
        info_gets + 1,
        "one info GET — the dirty path's shape, unchanged"
    );
}

/// A normal own-write churn (later second ⇒ fresh etag in the
/// response) takes exactly today's path: record, hash, heartbeat —
/// zero verify GETs, zero added requests.
#[tokio::test]
async fn s4b_normal_churn_write_adds_zero_requests() {
    let mut harness = Harness::new(doc("aaaa"), false).await;
    harness.server.set_second(ATTACH_SECOND);
    let _ = harness.cycle().await;
    let path = appdata_path(SELF);
    let puts_after_attach = harness.puts(&path);
    let gets_after_attach = harness.gets(&path);

    // A later-second write: the response etag differs from the token —
    // no retention signature in sight.
    harness.server.set_second(ATTACH_SECOND + 100);
    harness.set_local(doc("bbbb"));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { published, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert!(published);
    assert!(!forked(&harness, &path));
    assert_eq!(harness.puts(&path), puts_after_attach + 1);
    assert_eq!(
        harness.gets(&path),
        gets_after_attach,
        "no verify GET on the churned-etag path"
    );
}

/// Retention never rescues a genuinely stale token: a stale-guarded
/// PUT at the retained second still answers 412 (the model's rule and
/// the client's bounded re-resolution both hold).
#[tokio::test]
async fn s3u_retention_never_rescues_a_stale_token() {
    let mut harness = Harness::new(doc("aaaa"), false).await;
    harness.server.set_second(ATTACH_SECOND);
    let _ = harness.cycle().await;
    let path = appdata_path(SELF);

    // A sibling's retained write at the *same* second but a *different
    // length* churns the etag (the model's rule): this clone's token
    // is stale even though the second matches.
    harness.server.set_second(ATTACH_SECOND);
    harness.server.peer_write(&path, &doc_bytes(&doc("ccccc")));
    harness.set_local(doc("bbbb"));
    let verdict = harness.cycle().await;
    let CycleVerdict::Completed { published, .. } = verdict else {
        panic!("cycle must complete: {verdict:?}");
    };
    assert!(published);
    assert!(!forked(&harness, &path), "one bounded re-resolution wins");
    assert_eq!(
        harness.puts(&path),
        2,
        "the stale-guarded 412'd PUT and the refreshed re-PUT"
    );
}

// SPDX-License-Identifier: AGPL-3.0-only
//! The two-engine journal convergence journey (DR-8 chunk 3, plan §5
//! commit 6): two full engines (in-memory repo fakes, each with its own
//! frozen clock) and two files actors over **one shared** fake `WebDAV`
//! account. Deterministic by construction: `tokio::time::start_paused`
//! (auto-advance fires the poll timers), terminal state polled under a
//! wall-clock deadline — never a fixed sleep.
//!
//! The journey: A records X, B records Y → both converge to {X, Y};
//! B edits X → A converges to B's text; A deletes Y → B's Y is
//! tombstoned in place; concurrent equal-clock edits of X → both hold
//! the *same* winner (whichever the totaliser picks — equality, not
//! identity); terminal: equal homes, hash-equal published documents,
//! and a quiescent window with zero PUTs (convergence is stable, not
//! oscillating).

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::reflection::{ReflectionState, ReviewCadence};
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    Clock, CommandOutcome, NotifierMessage, PersistedState, ReflectionEntry, ReflectionId,
    StateCommand, SyncCommand, SyncReport, SystemEvent, UserDataReport,
};
use taskboard_state::{DayRollConfig, EngineHandle, spawn_state_engine_with_user_data};
use taskboard_sync_files::document::UserDocument;
use taskboard_sync_files::hash::StateHash;
use taskboard_sync_files_nextcloud::actor::{FilesSyncConfig, spawn_files_sync_actor};
use taskboard_sync_files_nextcloud::cycle::PublishedState;
use taskboard_sync_files_nextcloud::error::TransportError;
use taskboard_sync_files_nextcloud::etag::Etag;
use taskboard_sync_files_nextcloud::test_support::FakeFilesTransport;
use taskboard_sync_files_nextcloud::transport::{
    FileContent, FilesTransport, MkcolOutcome, PropfindListing, PutOutcome,
};

/// The poll cadence the actors run at (virtual time; auto-advance
/// fires it whenever the runtime idles).
const POLL: Duration = Duration::from_secs(1);

/// The shared logical instant of the concurrent-edit step.
const T5: i64 = 5_000;

/// The reviewed window every fixture entry carries (the mint checks
/// only the 24h span; the calendar is data).
fn day_window() -> (DateTime<Utc>, DateTime<Utc>) {
    let start = Utc.timestamp_opt(1_793_836_800, 0).unwrap();
    (start, start + chrono::TimeDelta::try_hours(24).unwrap())
}

/// A frozen logical clock, advanced explicitly per journey step.
#[derive(Debug)]
struct FrozenClock(Mutex<i64>);

impl FrozenClock {
    fn new(secs: i64) -> Arc<Self> {
        Arc::new(Self(Mutex::new(secs)))
    }

    fn advance_secs(&self, secs: i64) {
        *self.0.lock().expect("poisoned") += secs;
    }

    fn set(&self, secs: i64) {
        *self.0.lock().expect("poisoned") = secs;
    }
}

impl Clock for FrozenClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(*self.0.lock().expect("poisoned"), 0)
            .unwrap()
    }
}

/// An advancing heartbeat stamp (30 s per read) so consecutive stamps
/// stay distinct — deterministic, never the wall clock (AGENTS §5
/// clock discipline; the stamps feed freshness math, not assertions).
#[derive(Debug)]
struct StepClock(AtomicI64);

impl Clock for StepClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(self.0.fetch_add(30, Ordering::Relaxed), 0)
            .unwrap()
    }
}

/// Forwards one shared fake to both actors (the model is
/// `Arc`-shared mutable state; sharing is its purpose).
struct SharedFake {
    fake: Arc<FakeFilesTransport>,
}

impl FilesTransport for SharedFake {
    async fn mkcol(&self, path: &str) -> Result<MkcolOutcome, TransportError> {
        self.fake.mkcol(path).await
    }

    async fn propfind_devices(&self) -> Result<PropfindListing, TransportError> {
        self.fake.propfind_devices().await
    }

    async fn propfind_tree(&self, path: &str) -> Result<PropfindListing, TransportError> {
        self.fake.propfind_tree(path).await
    }

    async fn get(&self, path: &str) -> Result<FileContent, TransportError> {
        self.fake.get(path).await
    }

    async fn put_create(&self, path: &str, body: &[u8]) -> Result<PutOutcome, TransportError> {
        self.fake.put_create(path, body).await
    }

    async fn put_if_match(
        &self,
        path: &str,
        body: &[u8],
        etag: &Etag,
    ) -> Result<PutOutcome, TransportError> {
        self.fake.put_if_match(path, body, etag).await
    }
}

/// One full engine: in-memory repo, frozen clock, live user-data lane.
struct Side {
    engine: EngineHandle,
    /// The side's F1 author stamp (chunk 1): distinct per device, so
    /// the winner's author slot names the writer on every reader.
    author: taskboard_domain::ids::AuthorId,
    clock: Arc<FrozenClock>,
    user_data_tx: tokio::sync::mpsc::Sender<UserDataReport>,
    board_report_tx: tokio::sync::mpsc::Sender<SyncReport>,
    system: tokio::sync::broadcast::Sender<SystemEvent>,
    fake: Arc<FakeFilesTransport>,
    /// Keeps the engine's join handle alive for the journey's lifetime.
    _join: tokio::task::JoinHandle<()>,
}

async fn spawn_side(
    start_secs: i64,
    skip_ids: u64,
    fake: Arc<FakeFilesTransport>,
) -> (Side, tempfile::TempDir) {
    let repo = Arc::new(InMemoryRepository::with_state(PersistedState::default()));
    let ids = Arc::new(CountingIds::new());
    // Burn ids so two independent counters never mint the same first
    // reflection UUID (the collision would fuse the sides' entries).
    for _ in 0..skip_ids {
        ids.new_reflection_id();
    }
    let clock = FrozenClock::new(start_secs);
    let author = taskboard_domain::ids::AuthorId::from_uuid(uuid::Uuid::from_u128(
        u128::try_from(start_secs).expect("side seed"),
    ));
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (user_data_tx, user_data) = tokio::sync::mpsc::channel::<UserDataReport>(4);
    let (system, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (engine, join) = spawn_state_engine_with_user_data(
        repo,
        ids,
        clock.clone(),
        sync_out,
        report_rx,
        user_data,
        system.clone(),
        system_rx,
        notifier,
        taskboard_domain::SettingsSnapshot::default(),
        DayRollConfig::disabled(),
        Some(author),
    )
    .await
    .expect("engine boots");
    let dir = tempfile::tempdir().expect("tempdir");
    (
        Side {
            engine,
            author,
            clock,
            user_data_tx,
            board_report_tx: report_tx,
            system,
            fake,
            _join: join,
        },
        dir,
    )
}

impl Side {
    /// Records an entry at this side's current clock.
    async fn record(&self, text: &str) -> ReflectionEntry {
        self.clock.advance_secs(60);
        let CommandOutcome::ReflectionRecorded { entry } = self
            .engine
            .execute(StateCommand::RecordReflection {
                cadence: ReviewCadence::Day,
                day: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
                window: day_window(),
                text: text.to_owned(),
                mood: None,
                energy: None,
                flags: BTreeSet::new(),
            })
            .await
            .expect("accepted")
        else {
            panic!("wrong receipt");
        };
        entry
    }

    /// Edits an entry (a fresh mutation stamp from this side's clock).
    async fn edit(&self, id: ReflectionId, text: &str) {
        self.clock.advance_secs(60);
        self.engine
            .execute(StateCommand::UpdateReflection {
                id,
                text: Some(text.to_owned()),
                mood: None,
                energy: None,
                flags_add: BTreeSet::new(),
                flags_remove: BTreeSet::new(),
            })
            .await
            .expect("accepted");
    }

    /// Deletes an entry (tombstone-in-place at this side's clock).
    async fn delete(&self, id: ReflectionId) {
        self.clock.advance_secs(60);
        self.engine
            .execute(StateCommand::DeleteReflection { id })
            .await
            .expect("accepted");
    }

    fn home(&self) -> ReflectionState {
        self.engine.shared_state().load().reflection.clone()
    }

    /// Imports one roaming setting through this side's user-data lane
    /// (the actor's delivery seam — the report the files fold emits).
    async fn adopt_setting(&self, value: taskboard_domain::RoamingSettingValue, at: i64) {
        self.user_data_tx
            .send(UserDataReport {
                journal: Vec::new(),
                settings: vec![taskboard_domain::ImportedSetting {
                    value,
                    clock: self.clock.now() + chrono::TimeDelta::seconds(at),
                }],
                proofs: Vec::new(),
            })
            .await
            .expect("user-data lane open");
    }

    /// Delivers one reminder proof through this side's user-data lane.
    async fn deliver_proof(&self, proof: taskboard_domain::ReminderDelivery) {
        self.user_data_tx
            .send(UserDataReport {
                journal: Vec::new(),
                settings: Vec::new(),
                proofs: vec![proof],
            })
            .await
            .expect("user-data lane open");
    }

    fn roaming(&self) -> taskboard_domain::RoamingSettings {
        self.engine.shared_state().load().roaming.clone()
    }

    fn reminders(&self) -> taskboard_domain::ReminderState {
        self.engine.shared_state().load().reminders.clone()
    }

    /// Spawns this side's files actor over the shared account; the
    /// returned pair keeps the actor's join and status watch alive.
    fn spawn_actor(
        &self,
        device_json: std::path::PathBuf,
    ) -> (
        tokio::task::JoinHandle<()>,
        tokio::sync::watch::Receiver<taskboard_sync_files_nextcloud::status::FilesSyncStatus>,
    ) {
        spawn_files_sync_actor(
            SharedFake {
                fake: self.fake.clone(),
            },
            device_json,
            Arc::new(PublishedState::new(self.engine.shared_state())),
            Arc::new(self.engine.clone()),
            self.user_data_tx.clone(),
            self.board_report_tx.clone(),
            self.system.clone(),
            FilesSyncConfig {
                connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
                poll_interval: POLL,
                backoff_initial: Duration::from_millis(50),
                backoff_max: Duration::from_millis(200),
                clock: Arc::new(StepClock(1_793_800_000.into())),
                app_version: "convergence-journey".into(),
                deck_origins: std::collections::BTreeMap::from([(
                    taskboard_domain::ids::ConnectorId::from("nextcloud"),
                    DECK_ORIGIN.to_owned(),
                )]),
            },
        )
    }
}

/// Polls `pred` under a wall-clock deadline while the paused runtime
/// auto-advances the actors' poll timers (no fixed sleeps).
async fn until(label: &str, pred: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !pred() {
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(
            std::time::Instant::now() < deadline,
            "convergence never landed: {label}"
        );
    }
}

/// The minted device id: read back from the device.json the ceremony
/// wrote (polled — the actor writes it on its first loop turn; the
/// await yields the runtime so the actors make progress).
async fn device_id(dir: &std::path::Path) -> String {
    let path = dir.join("device.json");
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(id) = value["device_id"].as_str()
        {
            return id.to_owned();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(
            std::time::Instant::now() < deadline,
            "device.json never appeared"
        );
    }
}

/// The count of appdata PUTs across the whole account.
fn appdata_puts(fake: &Arc<FakeFilesTransport>, ids: &[String]) -> usize {
    ids.iter()
        .map(|id| fake.count("PUT", Some(&format!("taskboard/devices/{id}/appdata.json"))))
        .sum()
}

/// Polls until the fake serves the path, then returns the bytes.
async fn until_ready(fake: &Arc<FakeFilesTransport>, path: &str) -> Vec<u8> {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(bytes) = fake.file(path) {
            return bytes;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(
            std::time::Instant::now() < deadline,
            "{path} never published"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn two_engines_converge_their_journals_and_stay_quiet() {
    let fake = Arc::new(FakeFilesTransport::new());
    let (a, dir_a) = spawn_side(1_000, 0, fake.clone()).await;
    let (b, dir_b) = spawn_side(1_000, 1, fake.clone()).await;
    let _actors = (
        a.spawn_actor(dir_a.path().join("device.json")),
        b.spawn_actor(dir_b.path().join("device.json")),
    );
    let device_ids = [device_id(dir_a.path()).await, device_id(dir_b.path()).await];
    let appdata = |id: &str| format!("taskboard/devices/{id}/appdata.json");

    // Step 1 — A records X at T1, B records Y at T1; both converge to
    // {X, Y} (the fleet's per-entry clock LWW union).
    let x = a.record("x by a").await;
    let y = b.record("y by b").await;
    assert_ne!(x.id, y.id, "independent counters stay distinct");
    let (x_id, y_id) = (x.id, y.id);
    until("step1 entries", || {
        let (ha, hb) = (a.home(), b.home());
        ha.entries.contains_key(&x_id)
            && ha.entries.contains_key(&y_id)
            && hb.entries.contains_key(&x_id)
            && hb.entries.contains_key(&y_id)
    })
    .await;

    // Step 2 — B edits X at T3 (after receiving it): A converges to
    // B's text.
    b.edit(x_id, "x edited by b").await;
    until("step2 edit", || {
        a.home().entries[&x_id].text == "x edited by b"
            && b.home().entries[&x_id].text == "x edited by b"
    })
    .await;

    // Step 3 — A deletes Y at T4: B's Y is tombstoned in place (the
    // body stays; the wire grammar carries none).
    a.delete(y_id).await;
    until("step3 tombstone", || {
        let yb = &b.home().entries[&y_id];
        yb.deleted && yb.text == "y by b"
    })
    .await;

    // Step 4 — concurrent equal-clock edits of X: both engines are
    // parked at the same logical T5 and both edit; whichever row the
    // fold's totaliser picks, both sides hold the *same* one.
    a.clock.set(T5);
    b.clock.set(T5);
    b.edit(x_id, "b's concurrent pick").await;
    a.edit(x_id, "a's concurrent pick").await;
    until("step4 concurrent tie", || {
        a.home().entries[&x_id] == b.home().entries[&x_id]
    })
    .await;

    // Step 4b — D-BF3: the settings and proofs sections converge like
    // the journal does (the extracted scenario below).
    settings_and_proofs_converge(&a, &b).await;

    // Terminal assertions — equal homes (journals, settings, proofs),
    // hash-equal published files.
    until("terminal homes", || {
        let ha = a.home();
        let hb = b.home();
        ha == hb && ha.entries.len() == 2 && a.roaming() == b.roaming()
    })
    .await;
    let doc_a: UserDocument =
        serde_json::from_slice(&until_ready(&fake, &appdata(&device_ids[0])).await)
            .expect("published doc");
    let doc_b: UserDocument =
        serde_json::from_slice(&until_ready(&fake, &appdata(&device_ids[1])).await)
            .expect("published doc");
    let (hash_a, hash_b) = (StateHash::of(&doc_a), StateHash::of(&doc_b));
    assert_eq!(hash_a, hash_b, "both devices publish the same merged view");

    // Step 5 — quiescence: a further window of cycles records zero
    // appdata PUTs (convergence is stable, not oscillating).
    let puts_before = appdata_puts(&fake, &device_ids);
    let window = std::time::Instant::now() + Duration::from_millis(300);
    while std::time::Instant::now() < window {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        appdata_puts(&fake, &device_ids),
        puts_before,
        "the steady state is quiet: no further PUTs"
    );

    let _ = a.system.send(SystemEvent::Shutdown);
    let _ = b.system.send(SystemEvent::Shutdown);
}

/// The D-BF3 convergence scenario (the journey's step 4b): A adopts a
/// setting at 05:30, B (independently, newer clock) at 06:00 — the
/// fleet settles on B's row on both sides — and B's delivered proof
/// (a Rule-ref fact) reaches A: delivered-ness is membership, so A's
/// at-most-once guard learns the fleet settled that occurrence.
async fn settings_and_proofs_converge(a: &Side, b: &Side) {
    // Step 4b — D-BF3: the settings and proofs sections converge like
    // the journal does. A adopts the day start at 05:30, B (independ-
    // ently, with a newer clock) at 06:00: the fleet settles on B's
    // newer row on both sides. B's delivered proof (a Rule-ref fact)
    // reaches A: is-delivered is membership, so A's at-most-once guard
    // now knows the fleet settled that occurrence.
    a.adopt_setting(
        taskboard_domain::RoamingSettingValue::DayStartTime(
            chrono::NaiveTime::from_hms_opt(5, 30, 0).unwrap(),
        ),
        60,
    )
    .await;
    b.adopt_setting(
        taskboard_domain::RoamingSettingValue::DayStartTime(
            chrono::NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
        ),
        120,
    )
    .await;
    let rule = taskboard_domain::ReminderRuleId::from(uuid::Uuid::from_u128(0x2f_1a));
    b.deliver_proof(taskboard_domain::ReminderDelivery {
        reminder: taskboard_domain::ReminderRef::Rule(rule),
        occurrence: chrono::DateTime::from_timestamp(1_900_000_000, 0).unwrap(),
        delivered_at: b.clock.now(),
    })
    .await;
    let occurrence = chrono::DateTime::from_timestamp(1_900_000_000, 0).unwrap();
    let settled = taskboard_domain::RoamingSettingValue::DayStartTime(
        chrono::NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
    );
    until("step4b settings+proofs", || {
        let (ra, rb) = (a.roaming(), b.roaming());
        let (ia, ib) = (a.reminders(), b.reminders());
        let six = |home: &taskboard_domain::RoamingSettings| {
            home.0
                .get("day_start_time")
                .is_some_and(|row| row.value == settled)
        };
        let rule_ref = taskboard_domain::ReminderRef::Rule(rule);
        six(&ra)
            && six(&rb)
            && ia.is_delivered(&rule_ref, &occurrence)
            && ib.is_delivered(&rule_ref, &occurrence)
    })
    .await;
}

/// The board-over-files journey (dispatch chunk 5): A creates and
/// binds a files board, drains its dirty entities into a chunk; B
/// discovers the foreign subtree, folds first (publishing nothing
/// until then), adopts via the engine's binding mint, edits the task;
/// both engines converge to one board state — through the ordinary
/// per-field LWW pipeline, with no Deck actor anywhere (the
/// no-sidecar posture is the journey's shape: no anchor card can
/// exist because no Deck lane exists to mint one).
#[tokio::test(start_paused = true)]
#[allow(clippy::too_many_lines)] // one journey: create, bind, drain, discover, adopt, rename, converge
async fn two_engines_converge_a_files_board() {
    use taskboard_domain::{CommandOutcome, StateCommand};

    let fake = Arc::new(FakeFilesTransport::new());
    fake.seed_dirs(&["taskboard"]);
    let (a, dir_a) = spawn_side(1_000, 10, fake.clone()).await;
    let (b, dir_b) = spawn_side(1_000, 20, fake.clone()).await;

    // A: board + stack + card, bound to its files connector.
    let board_uuid = uuid::Uuid::from_u128(0xB0A1D);
    let CommandOutcome::CreatedBoard(board) = a
        .engine
        .execute(StateCommand::CreateBoard {
            title: "shared board".into(),
            color: taskboard_domain::Color::new("2a9d8f"),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedStack(stack) = a
        .engine
        .execute(StateCommand::CreateStack {
            board,
            title: "column".into(),
            order: 1,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task) = a
        .engine
        .execute(StateCommand::CreateTask {
            title: "seed card".into(),
            stack,
            order: 1,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    a.engine
        .execute(StateCommand::BindBoard {
            board,
            remote: taskboard_domain::RemoteBoardId::files(board_uuid),
            connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
        })
        .await
        .expect("accepted");

    // B starts binding-less: the discovery door is the journey's
    // point (no opt-in command on B's side).
    let _actors_a = a.spawn_actor(dir_a.path().join("device.json"));
    let a_device = device_id(dir_a.path()).await;
    // The chunk's file name is its range's last seq (chunk 4's
    // grammar); wait for any ops file in A's subtree.
    let a_ops_prefix = format!("taskboard/boards/{board_uuid}/devices/{a_device}/ops/");
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !fake
            .requests()
            .iter()
            .any(|r| r.method == "PUT" && r.path.starts_with(&a_ops_prefix))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert!(
                std::time::Instant::now() < deadline,
                "a's chunk never published; requests: {:?}",
                fake.requests()
                    .iter()
                    .map(|r| format!("{} {}", r.method, r.path))
                    .collect::<Vec<_>>()
            );
        }
    }
    until("a's outbox drains", || {
        a.engine
            .shared_state()
            .load()
            .sync
            .get(board)
            .is_none_or(|row| row.pending_ops == 0)
    })
    .await;

    // B: the discovery fold-first adoption — B's actor finds the
    // foreign subtree, folds it, and the engine mints the binding.
    let _actors_b = b.spawn_actor(dir_b.path().join("device.json"));
    let task_title = |side: &Side, task: taskboard_domain::TaskId| {
        side.engine
            .shared_state()
            .load()
            .tasks
            .get(&task)
            .map(|t| t.title.clone())
    };
    until("b adopts the board", || {
        let state = b.engine.shared_state().load();
        state.boards.get(&board).is_some_and(|row| {
            row.remote.as_ref().is_some_and(|binding| {
                binding.connector.as_str() == "files-sync"
                    && binding.board.as_files() == Some(board_uuid)
            })
        }) && task_title(&b, task).as_deref() == Some("seed card")
    })
    .await;

    // B edits the card; A converges (per-field LWW, the same
    // pipeline a Deck snapshot takes).
    // Re-fetch B's local id for A's task (identity on files is the
    // domain UUID, so B's row IS A's row).
    let shared_task = {
        let state = b.engine.shared_state().load();
        state
            .tasks
            .values()
            .find(|t| t.title == "seed card")
            .map(|t| t.id)
            .expect("B adopted the task")
    };
    // A strictly later stamp: the rename wins its duel deterministically.
    b.clock.advance_secs(60);
    b.engine
        .execute(StateCommand::UpdateTask {
            id: shared_task,
            changes: taskboard_domain::TaskChanges {
                title: Some("renamed by b".into()),
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .await
        .expect("accepted");

    // Terminal pins — one board state on both engines:
    // 1. B's rename converged: both sides hold the same winning title.
    // 2. The winner's author leg (chunk 1): the title's author slot is
    //    B's author on BOTH readers — the stamp rode the chunk and the
    //    snapshot end to end (B's strictly later clock picks the row;
    //    the author proves which device's write survived).
    until("a holds b's rename with b's author", || {
        a.engine
            .shared_state()
            .load()
            .tasks
            .get(&shared_task)
            .is_some_and(|t| t.title == "renamed by b" && t.authors.title == Some(b.author))
    })
    .await;
    until("b holds its own rename with its author", || {
        b.engine
            .shared_state()
            .load()
            .tasks
            .get(&shared_task)
            .is_some_and(|t| t.title == "renamed by b" && t.authors.title == Some(b.author))
    })
    .await;
    // 3. No Deck-tagged remote ref exists anywhere (the no-sidecar
    //    posture — no anchor card, no Deck echo, no attachment lane).
    // 4. The outboxes drain: the fleet went quiet.
    until("outboxes drain and the fleet went quiet", || {
        let (sa, sb) = (
            a.engine.shared_state().load(),
            b.engine.shared_state().load(),
        );
        let no_deck = sa.tasks.values().chain(sb.tasks.values()).all(|t| {
            t.remote
                .as_ref()
                .is_none_or(|r| r.board.as_files().is_some())
        });
        let quiet = [sa.sync.iter(), sb.sync.iter()]
            .into_iter()
            .flatten()
            .all(|(_, row)| row.pending_ops == 0);
        no_deck && quiet
    })
    .await;
}

const DECK_ORIGIN: &str = "https://cloud.example";
const DECK_BOARD: u64 = 42;
const DECK_STACK: u64 = 7;
const DECK_CARD: u64 = 70;

fn deck_snapshot(board: u64, card_title: &str, modified_secs: i64) -> SyncReport {
    use chrono::TimeZone as _;
    use taskboard_domain::{
        RemoteBoard, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteStack, RemoteStackId,
        RemoteStackRef, RemoteTask,
    };
    let at = chrono::Utc
        .timestamp_opt(modified_secs, 0)
        .single()
        .expect("in range");
    let remote_board = RemoteBoardId::deck(board);
    let stack = RemoteStackId::deck(DECK_STACK + board);
    SyncReport::Completed {
        connector: taskboard_domain::ids::ConnectorId::from("nextcloud"),
        snapshot: Box::new(taskboard_domain::RemoteBoardSnapshot::Deck(
            taskboard_domain::DeckBoardSnapshot {
                board: RemoteBoard {
                    id: remote_board,
                    title: "moved".into(),
                    color: "2a9d8f".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: at,
                },
                stacks: vec![RemoteStack {
                    id: RemoteStackRef {
                        board: remote_board,
                        stack,
                    },
                    title: "column".into(),
                    order: 1,
                    archived: false,
                    deleted_at: None,
                    last_modified: at,
                }],
                tasks: vec![RemoteTask {
                    id: RemoteCardRef {
                        board: remote_board,
                        stack,
                        card: RemoteCardId::deck(DECK_CARD + board),
                    },
                    title: card_title.into(),
                    description: String::new(),
                    duedate: None,
                    scheduled: None,
                    done: None,
                    stack,
                    order: 1,
                    labels: std::collections::BTreeSet::new(),
                    archived: false,
                    last_modified: at,
                    created_at: at,
                }],
                labels: vec![],
            },
        )),
        validators: taskboard_domain::BoardPullValidators {
            boards: taskboard_domain::SyncValidators::default(),
            stacks: taskboard_domain::SyncValidators::default(),
            archived_stacks: taskboard_domain::SyncValidators::default(),
        },
        pushes: vec![],
        read_at: at,
        duration: Duration::ZERO,
        app_data: taskboard_domain::AppDataEvidence::Absent,
        features: taskboard_domain::DeckFeatures::default(),
    }
}

/// A side holding the Deck board, as every device does before a move.
async fn bind_to_deck(side: &Side, board_number: u64) -> taskboard_domain::BoardId {
    use taskboard_domain::{CommandOutcome, StateCommand};
    let CommandOutcome::CreatedBoard(board) = side
        .engine
        .execute(StateCommand::CreateBoard {
            title: "moved".into(),
            color: taskboard_domain::Color::new("2a9d8f"),
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    side.engine
        .execute(StateCommand::BindBoard {
            board,
            remote: taskboard_domain::RemoteBoardId::deck(board_number),
            connector: taskboard_domain::ids::ConnectorId::from("nextcloud"),
        })
        .await
        .expect("accepted");
    side.board_report_tx
        .send(deck_snapshot(board_number, "seed card", 500))
        .await
        .expect("engine live");
    until("the Deck card lands", || {
        let state = side.engine.shared_state().load();
        state
            .stacks
            .values()
            .filter(|s| s.board == board)
            .any(|s| state.tasks.values().any(|t| t.stack == s.id))
    })
    .await;
    board
}

/// The mover's four steps over the fake tier (the live `AppWorld` is
/// the docker journey's subject); the machine itself is unit-pinned.
async fn run_mover(
    a: &Side,
    board: taskboard_domain::BoardId,
    device: &std::path::Path,
) -> uuid::Uuid {
    use taskboard_domain::StateCommand;
    use taskboard_sync_files::board_move::{MOVE_RECORD_FORMAT, MoveRecord, MovedFrom};
    use taskboard_sync_files_nextcloud::board_cycle::{publish_move_floor, publish_move_record};
    let clock: Arc<dyn taskboard_domain::Clock> = Arc::new(StepClock(1_793_700_000.into()));
    let identity = taskboard_sync_files_nextcloud::device::load_or_create(device, clock.as_ref())
        .await
        .expect("device");
    let context = taskboard_sync_files_nextcloud::cycle::SyncContext {
        identity: identity.clone(),
        app_version: "mover".into(),
        clock,
        local: Arc::new(PublishedState::new(a.engine.shared_state())),
        report_tx: a.user_data_tx.clone(),
        connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
        state_reader: Arc::new(a.engine.clone()),
        board_reports: a.board_report_tx.clone(),
        deck_origins: std::collections::BTreeMap::new(),
    };
    let moved_from = MovedFrom {
        deck_server: DECK_ORIGIN.to_owned(),
        deck_board: DECK_BOARD,
    };
    let persisted = taskboard_domain::SyncStateReader::read_state(&a.engine)
        .await
        .expect("reads");
    let remote = board.as_uuid();
    let transport = SharedFake {
        fake: a.fake.clone(),
    };
    publish_move_floor(&transport, &context, board, remote, &persisted, &moved_from)
        .await
        .expect("floor");
    a.engine
        .execute(StateCommand::UnbindBoard { board })
        .await
        .expect("retired");
    let state = a.engine.shared_state().load_full();
    let stacks = state
        .stacks
        .values()
        .filter(|s| s.board == board)
        .filter_map(|s| Some((s.remote?.stack.as_deck()?, s.id.as_uuid())))
        .collect();
    let cards = state
        .tasks
        .values()
        .filter_map(|t| Some((t.remote?.card.as_deck()?, t.id.as_uuid())))
        .collect();
    let record = MoveRecord {
        format_version: MOVE_RECORD_FORMAT,
        moved_from,
        moved_by: identity.device_id,
        committed_at: chrono::DateTime::UNIX_EPOCH,
        stacks,
        cards,
        labels: std::collections::BTreeMap::new(),
    };
    publish_move_record(&transport, remote, &record)
        .await
        .expect("commit");
    a.engine
        .execute(StateCommand::BindBoard {
            board,
            remote: taskboard_domain::RemoteBoardId::files(remote),
            connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
        })
        .await
        .expect("assigned");
    remote
}

fn titles(side: &Side, board: taskboard_domain::BoardId) -> Vec<String> {
    let state = side.engine.shared_state().load();
    let mut titles: Vec<String> = state
        .tasks
        .values()
        .filter(|t| !t.deleted)
        .filter(|t| state.stacks.get(&t.stack).is_some_and(|s| s.board == board))
        .map(|t| t.title.clone())
        .collect();
    titles.sort();
    titles
}

/// Two devices on Deck; B is offline through A's move. On return B
/// raises the finding, re-keys onto A's ids, folds first, and its
/// offline work reaches A with nothing duplicated or tombstoned.
#[tokio::test(start_paused = true)]
async fn an_offline_device_adopts_a_moved_board_with_its_edits() {
    use taskboard_domain::{StateCommand, TaskChanges};

    let fake = Arc::new(FakeFilesTransport::new());
    fake.seed_dirs(&["taskboard"]);
    let (a, dir_a) = spawn_side(1_000, 10, fake.clone()).await;
    let (b, dir_b) = spawn_side(1_000, 20, fake.clone()).await;
    let board_a = bind_to_deck(&a, DECK_BOARD).await;
    let board_b = bind_to_deck(&b, DECK_BOARD).await;
    assert_ne!(board_a, board_b);

    let remote = run_mover(&a, board_a, &dir_a.path().join("device.json")).await;
    let _actors_a = a.spawn_actor(dir_a.path().join("device.json"));

    let (card_b, stack_b) = {
        let state = b.engine.shared_state().load();
        let task = state
            .tasks
            .values()
            .find(|t| state.stacks[&t.stack].board == board_b)
            .expect("b's copy");
        (task.id, task.stack)
    };
    b.clock.advance_secs(60);
    b.engine
        .execute(StateCommand::UpdateTask {
            id: card_b,
            changes: TaskChanges {
                title: Some("renamed offline".into()),
                ..TaskChanges::default()
            },
        })
        .await
        .expect("accepted");
    b.engine
        .execute(StateCommand::CreateTask {
            title: "created offline".into(),
            stack: stack_b,
            order: 2,
        })
        .await
        .expect("accepted");

    let (_actors_b, status_b) = b.spawn_actor(dir_b.path().join("device.json"));
    let moved = taskboard_domain::BoardId::from_uuid(remote);
    until("b's board becomes the mover's", || {
        let state = b.engine.shared_state().load();
        !state.boards.contains_key(&board_b)
            && state.boards.get(&moved).is_some_and(|row| {
                row.remote
                    .as_ref()
                    .is_some_and(|r| r.board.as_files() == Some(remote))
            })
    })
    .await;

    let want = vec!["created offline".to_owned(), "renamed offline".to_owned()];
    until("both sides hold exactly b's work", || {
        titles(&a, board_a) == want && titles(&b, moved) == want
    })
    .await;
    until("the finding settles to adopted", || {
        matches!(
            status_b.borrow().moved_boards.get(&remote),
            Some(taskboard_sync_files_nextcloud::status::BoardMoveFinding::Adopted { .. })
        )
    })
    .await;

    // A Deck cycle already in flight when adoption landed folds nothing.
    b.board_report_tx
        .send(deck_snapshot(DECK_BOARD, "stale deck echo", 900))
        .await
        .expect("engine live");
    // Reports are FIFO: a files row behind the stale report proves the
    // engine already ingested it.
    let mut marked = b.engine.shared_state().load().boards[&moved].clone();
    marked.title = "sentinel".into();
    marked.clocks.title = chrono::DateTime::<chrono::Utc>::MAX_UTC;
    let mut snapshot = taskboard_domain::remote::FilesBoardSnapshot::identity_only(
        taskboard_domain::RemoteBoardId::files(remote),
    );
    snapshot.board_row = Some(marked);
    b.board_report_tx
        .send(SyncReport::Completed {
            connector: taskboard_domain::ids::ConnectorId::from("files-sync"),
            snapshot: Box::new(taskboard_domain::RemoteBoardSnapshot::Files(snapshot)),
            validators: taskboard_domain::BoardPullValidators {
                boards: taskboard_domain::SyncValidators::default(),
                stacks: taskboard_domain::SyncValidators::default(),
                archived_stacks: taskboard_domain::SyncValidators::default(),
            },
            pushes: vec![],
            read_at: chrono::DateTime::UNIX_EPOCH,
            duration: Duration::ZERO,
            app_data: taskboard_domain::AppDataEvidence::Absent,
            features: taskboard_domain::DeckFeatures::default(),
        })
        .await
        .expect("engine live");
    until("the row behind the stale report lands", || {
        b.engine.shared_state().load().boards[&moved].title == "sentinel"
    })
    .await;
    assert_eq!(titles(&b, moved), want);
    let state = b.engine.shared_state().load();
    assert_eq!(state.boards.values().filter(|row| !row.deleted).count(), 1);
}

/// An edit B already pushed to Deck leaves no queued op; B's full-state
/// boot snapshot is what carries it onto the files tier.
#[tokio::test(start_paused = true)]
async fn an_edit_already_pushed_to_deck_still_reaches_files() {
    use taskboard_domain::{StateCommand, TaskChanges};

    let fake = Arc::new(FakeFilesTransport::new());
    fake.seed_dirs(&["taskboard"]);
    let (a, dir_a) = spawn_side(1_000, 10, fake.clone()).await;
    let (b, dir_b) = spawn_side(1_000, 20, fake.clone()).await;
    let board_a = bind_to_deck(&a, DECK_BOARD).await;
    let board_b = bind_to_deck(&b, DECK_BOARD).await;
    let remote = run_mover(&a, board_a, &dir_a.path().join("device.json")).await;
    let _actors_a = a.spawn_actor(dir_a.path().join("device.json"));

    let card_b = {
        let state = b.engine.shared_state().load();
        state
            .tasks
            .values()
            .find(|t| state.stacks[&t.stack].board == board_b)
            .expect("b's copy")
            .id
    };
    b.clock.advance_secs(60);
    b.engine
        .execute(StateCommand::UpdateTask {
            id: card_b,
            changes: TaskChanges {
                title: Some("pushed to deck".into()),
                ..TaskChanges::default()
            },
        })
        .await
        .expect("accepted");
    let queued = taskboard_domain::SyncStateReader::read_state(&b.engine)
        .await
        .expect("reads")
        .outbox;
    let mut report = deck_snapshot(DECK_BOARD, "pushed to deck", 1_060);
    if let SyncReport::Completed { pushes, .. } = &mut report {
        *pushes = queued
            .iter()
            .map(|pending| taskboard_domain::remote::PushOutcome {
                op: pending.op_id,
                result: taskboard_domain::remote::PushResult::Applied { echo: None },
            })
            .collect();
    }
    b.board_report_tx.send(report).await.expect("engine live");
    until("b's op settles", || {
        b.engine
            .shared_state()
            .load()
            .sync
            .get(board_b)
            .is_some_and(|row| row.pending_ops == 0)
    })
    .await;

    let _actors_b = b.spawn_actor(dir_b.path().join("device.json"));
    until("a holds the pushed edit once", || {
        titles(&a, board_a) == vec!["pushed to deck".to_owned()]
    })
    .await;
    let moved = taskboard_domain::BoardId::from_uuid(remote);
    assert_eq!(titles(&b, moved), vec!["pushed to deck".to_owned()]);
}

/// One connector, two boards: each board's report answers for that
/// board alone.
#[tokio::test(start_paused = true)]
async fn a_deck_report_leaves_another_boards_cards_alone() {
    let fake = Arc::new(FakeFilesTransport::new());
    let (side, _dir) = spawn_side(1_000, 10, fake).await;
    let first = bind_to_deck(&side, DECK_BOARD).await;
    let second = bind_to_deck(&side, 99).await;
    assert_eq!(titles(&side, first), vec!["seed card".to_owned()]);
    assert_eq!(titles(&side, second), vec!["seed card".to_owned()]);
}

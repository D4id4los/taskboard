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
    clock: Arc<FrozenClock>,
    user_data_tx: tokio::sync::mpsc::Sender<UserDataReport>,
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
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
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
        None,
    )
    .await
    .expect("engine boots");
    let dir = tempfile::tempdir().expect("tempdir");
    (
        Side {
            engine,
            clock,
            user_data_tx,
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
            self.user_data_tx.clone(),
            self.system.clone(),
            FilesSyncConfig {
                poll_interval: POLL,
                backoff_initial: Duration::from_millis(50),
                backoff_max: Duration::from_millis(200),
                clock: Arc::new(StepClock(1_793_800_000.into())),
                app_version: "convergence-journey".into(),
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

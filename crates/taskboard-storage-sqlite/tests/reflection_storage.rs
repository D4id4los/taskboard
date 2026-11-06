// SPDX-License-Identifier: MIT OR Apache-2.0
//! The reflection entries' storage contract (0022, DR-7 D-J13, #637):
//! codec round-trips (open + tombstoned rows, NULL ratings, empty flag
//! sets, parked unknown names), the strict decode rejections (typed
//! `Corrupted`, never a panic or a clamp), and the fake-repo parity —
//! `actions_from_state`'s reflection arm applies and loads identically
//! through `open_memory`. `TaskRepository::load` is the single read
//! path; corruption is planted via a hand-written raw INSERT on a raw
//! pool.

use std::collections::BTreeSet;

use chrono::TimeZone;

use taskboard_domain::idgen::IdGenerator as _;
use taskboard_domain::persistence::{PersistedState, TaskRepository};
use taskboard_domain::reflection::{
    HealthFlag, Rating, ReflectionAttributes, ReflectionEntry, ReviewCadence,
};
use taskboard_domain::test_support::CountingIds;
use taskboard_storage_sqlite::{SqliteTaskRepository, open, open_memory};

mod common;

fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn entry(ids: &CountingIds, deleted: bool) -> ReflectionEntry {
    ReflectionEntry {
        id: ids.new_reflection_id(),
        cadence: ReviewCadence::Day,
        day: day(2026, 11, 3),
        window_start: ts(100),
        window_end: ts(100 + 24 * 3600),
        text: "the day closed loud".to_owned(),
        attributes: ReflectionAttributes {
            mood: Rating::try_new(3).ok(),
            energy: Rating::try_new(4).ok(),
            flags: BTreeSet::from([HealthFlag::Migraine, HealthFlag::PoorSleep]),
            extra_flags: BTreeSet::from(["newer-flag".to_owned()]),
        },
        created_at: ts(100),
        updated_at: ts(150),
        deleted,
    }
}

/// Apply → load round-trips every field, tombstones included, and a
/// second apply (the edit path's shape) overwrites all columns.
#[tokio::test]
async fn entries_round_trip_including_the_tombstone_and_parked_flags() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let ids = CountingIds::default();
    let live = entry(&ids, false);
    let gone = entry(&ids, true);

    repo.apply(vec![
        taskboard_domain::PersistenceAction::UpsertReflectionEntry(live.clone()),
        taskboard_domain::PersistenceAction::UpsertReflectionEntry(gone.clone()),
    ])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.reflection.entries.len(), 2, "tombstones load too");
    assert_eq!(loaded.reflection.entries[&live.id], live);
    assert_eq!(loaded.reflection.entries[&gone.id], gone);
    assert!(loaded.reflection.entries[&gone.id].deleted);

    // The edit path: a whole-row upsert overwrites every column
    // (text, ratings, flags, tombstone flip, clocks).
    let edited = ReflectionEntry {
        cadence: ReviewCadence::Month,
        day: day(2026, 11, 1),
        window_start: ts(200),
        window_end: ts(200 + 30 * 24 * 3600),
        text: String::new(),
        attributes: ReflectionAttributes {
            mood: None,
            energy: None,
            flags: BTreeSet::new(),
            extra_flags: BTreeSet::new(),
        },
        created_at: live.created_at,
        updated_at: ts(300),
        deleted: true,
        ..live.clone()
    };
    repo.apply(vec![
        taskboard_domain::PersistenceAction::UpsertReflectionEntry(edited.clone()),
    ])
    .await
    .expect("apply the edit");
    let loaded = repo.load().await.expect("reload");
    assert_eq!(loaded.reflection.entries.get(&live.id), Some(&edited));
    repo.close().await;
}

/// Every cadence's text form round-trips (the stored column *is* the
/// serde unit name).
#[tokio::test]
async fn cadence_text_forms_round_trip() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let ids = CountingIds::default();
    for (index, cadence) in [
        (0_i64, ReviewCadence::Day),
        (1, ReviewCadence::Week),
        (2, ReviewCadence::Month),
    ] {
        let row = ReflectionEntry {
            id: ids.new_reflection_id(),
            cadence,
            window_start: ts(1000 + index * 1000),
            window_end: ts(2000 + index * 1000),
            ..entry(&ids, false)
        };
        repo.apply(vec![
            taskboard_domain::PersistenceAction::UpsertReflectionEntry(row.clone()),
        ])
        .await
        .expect("apply");
    }
    let loaded = repo.load().await.expect("load");
    let cadences: BTreeSet<ReviewCadence> = loaded
        .reflection
        .entries
        .values()
        .map(|e| e.cadence)
        .collect();
    assert_eq!(
        cadences,
        BTreeSet::from([
            ReviewCadence::Day,
            ReviewCadence::Week,
            ReviewCadence::Month
        ]),
    );
    repo.close().await;
}

/// A generated state's reflection section applies and loads
// identically (the fake-repo parity: the in-memory interpreter and
// sqlite produce equal maps).
#[tokio::test]
async fn generated_states_apply_and_load_identically() {
    let state = PersistedState {
        reflection: taskboard_domain::ReflectionState {
            entries: {
                let ids = CountingIds::default();
                let mut map = std::collections::BTreeMap::new();
                let live = entry(&ids, false);
                let gone = entry(&ids, true);
                map.insert(live.id, live);
                map.insert(gone.id, gone);
                map
            },
        },
        ..PersistedState::default()
    };
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    repo.apply(common::actions_from_state(&state))
        .await
        .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.reflection, state.reflection);
    repo.close().await;
}

/// Plants one corrupt cell per rejection class via a raw INSERT; every
/// decode failure is typed `Corrupted` — never a panic, never a clamp.
#[tokio::test]
async fn corrupt_cells_decode_to_corrupted() {
    let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    {
        let raw =
            sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
                .await
                .expect("raw connect");
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        for (suffix, day, cadence, start, end, mood, flags) in [
            (
                "bad-id",
                "2026-11-03",
                "day",
                "not-a-stamp",
                "2026-11-04T00:00:00+00:00",
                "3",
                "[]",
            ),
            (
                "bad-day",
                "03/11/2026",
                "day",
                "2026-11-03T00:00:00+00:00",
                "2026-11-04T00:00:00+00:00",
                "3",
                "[]",
            ),
            (
                "bad-cadence",
                "2026-11-03",
                "yearly",
                "2026-11-03T00:00:00+00:00",
                "2026-11-04T00:00:00+00:00",
                "3",
                "[]",
            ),
            (
                "bad-stamp",
                "2026-11-03",
                "day",
                "2026-11-03T00:00:00+00:00",
                "yesterday",
                "3",
                "[]",
            ),
            (
                "bad-rating",
                "2026-11-03",
                "day",
                "2026-11-03T00:00:00+00:00",
                "2026-11-04T00:00:00+00:00",
                "6",
                "[]",
            ),
            (
                "bad-flags-json",
                "2026-11-03",
                "day",
                "2026-11-03T00:00:00+00:00",
                "2026-11-04T00:00:00+00:00",
                "3",
                "{",
            ),
        ] {
            sqlx::query(
                r"INSERT INTO reflection_entries (entry_id, day, cadence, window_start,
                       window_end, text, mood, energy, flags, extra_flags, created_at,
                       updated_at, deleted)
                   VALUES (?, ?, ?, ?, ?, '', ?, NULL, ?, '[]',
                           '2026-11-03T00:00:00+00:00', '2026-11-03T00:00:00+00:00', 0)",
            )
            .bind(format!("00000000-0000-7100-8000-00000000-{suffix}"))
            .bind(day)
            .bind(cadence)
            .bind(start)
            .bind(end)
            .bind(mood)
            .bind(flags)
            .execute(&raw)
            .await
            .expect("plant the corrupt row");
        }
        raw.close().await;
    }

    let repo: SqliteTaskRepository = open(
        file.path(),
        taskboard_domain::ConnectorId::from("nextcloud"),
    )
    .await
    .expect("open");
    let err = repo.load().await.expect_err("corruption is typed");
    assert_eq!(
        err,
        taskboard_domain::persistence::RepositoryError::Corrupted,
        "every planted cell rejects typed"
    );
    repo.close().await;
}

/// A stored rating of 0 (outside the mint) is a data error — the
/// strict-decode posture, never a clamp to the representable range.
#[tokio::test]
async fn a_stored_zero_rating_is_corruption_not_a_clamp() {
    let file = tempfile::Builder::new().suffix(".db").tempfile().unwrap();
    {
        let raw =
            sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", file.path().display()))
                .await
                .expect("raw connect");
        sqlx::migrate!("./migrations")
            .run(&raw)
            .await
            .expect("migrate");
        sqlx::query(
            r"INSERT INTO reflection_entries (entry_id, day, cadence, window_start,
                   window_end, text, mood, energy, flags, extra_flags, created_at,
                   updated_at, deleted)
               VALUES ('00000000-0000-7100-8000-00000000-0000', '2026-11-03', 'day',
                       '2026-11-03T00:00:00+00:00', '2026-11-04T00:00:00+00:00', '',
                       0, NULL, '[]', '[]',
                       '2026-11-03T00:00:00+00:00', '2026-11-03T00:00:00+00:00', 0)",
        )
        .execute(&raw)
        .await
        .expect("plant the row");
        raw.close().await;
    }
    let repo = open(
        file.path(),
        taskboard_domain::ConnectorId::from("nextcloud"),
    )
    .await
    .expect("open");
    let err = repo.load().await.expect_err("the zero rating rejects");
    assert_eq!(
        err,
        taskboard_domain::persistence::RepositoryError::Corrupted
    );
    repo.close().await;
}

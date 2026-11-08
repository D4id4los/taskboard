// SPDX-License-Identifier: MIT OR Apache-2.0
//! The board lifecycle's pure decisions (design D-BF7, backbone
//! §7.2): the snapshot cadence, the chunk-seq allocator, the GC
//! eligibility predicate, and the late-join floor. One home for the
//! lifecycle's numbers and one home for its laws — **execution** (the
//! actual file deletions, the publish plumbing) is the transport's,
//! chunk 5; everything here is a pure function over data.
//!
//! The GC contract — **owner-scoped ∧ watermark-bounded**, both legs
//! load-bearing:
//! - *Owner-scoped*: the predicate's only input is the device's own
//!   chunk seq set ([`gc_eligible`]); a foreign device's files are not
//!   deletable by this API shape at all (the path grammar makes them
//!   unrepresentable here, and the transport maps each seq back into
//!   the owner's own `ops/` directory only).
//! - *Watermark-bounded*: only seqs `≤` a **published** snapshot's
//!   `seq` are eligible; `None` deletes nothing. Publishing a snapshot
//!   subsumes the owner's prior chunks (the fold is idempotent), which
//!   is exactly the predicate's safety law (proptested as P2: deleting
//!   the eligible set from any fold input never changes the fold).
//!
//! **Tombstones are forever**: deletion is never a tombstone GC — a
//! tombstone is a `deleted` *field* inside an entity payload and rides
//! snapshots like any field; GC removes only whole ops files at/below
//! the watermark. A horizon knob is documented future debt (backbone
//! §7.2), not v1. Snapshots themselves are never GC'd by this
//! machinery (retiring a device subtree is the runbook's user
//! procedure).

use std::collections::BTreeSet;

/// Snapshots publish on boot and after every N chunks (D-BF7).
pub const SNAPSHOT_EVERY_N_CHUNKS: u64 = 16;

/// The chunk/snapshot envelope format this version writes (the
/// transport gates on it; the codecs decode any value).
pub const BOARD_CHUNK_FORMAT: u32 = 1;

/// Whether a snapshot is due: on every process boot (the backbone's
/// self-healing and the move ceremony's "first snapshot = converged
/// state" both lean on it) or once `SNAPSHOT_EVERY_N_CHUNKS` chunks
/// have accrued since the last one.
#[must_use]
pub const fn snapshot_due(chunks_since_last_snapshot: u64, is_boot: bool) -> bool {
    is_boot || chunks_since_last_snapshot >= SNAPSHOT_EVERY_N_CHUNKS
}

/// The next chunk's seq range over `entries` entity writes:
/// gap-free and monotonic per (board, device) — `first = last_prev +
/// 1`, `last = first + entries - 1`. `None` for zero entries: a chunk
/// with no entities is never published (no empty ops files — the
/// illegal state is unrepresentable).
#[must_use]
pub const fn next_chunk_range(last_published: Option<u64>, entries: usize) -> Option<(u64, u64)> {
    let first = match last_published {
        None => 1,
        Some(last) => last + 1,
    };
    if entries == 0 {
        return None;
    }
    let count = entries as u64;
    match first.checked_add(count - 1) {
        Some(last) => Some((first, last)),
        // u64 exhaustion on a single board/device lane is beyond every
        // realistic fleet lifetime, but it is a typed refusal, not a
        // wraparound.
        None => None,
    }
}

/// The device's own chunk seqs deletable once `snapshot-<watermark>`
/// is published: exactly the seqs `≤` the watermark. A `None`
/// watermark (no published snapshot) deletes nothing — the pinned
/// leg. Tombstoned entities confer no exemption: whole ops files go,
/// their tombstone fields living on in the snapshots.
#[must_use]
pub fn gc_eligible(own_chunk_seqs: &BTreeSet<u64>, published_watermark: Option<u64>) -> Vec<u64> {
    match published_watermark {
        None => Vec::new(),
        Some(watermark) => own_chunk_seqs
            .iter()
            .copied()
            .filter(|seq| *seq <= watermark)
            .collect(),
    }
}

/// One device's replay instructions after the late-join floor: the
/// newest snapshot is the floor; chunks `≤` the floor are subsumed
/// (skipped), chunks `>` the floor replay in the fold.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceReplay {
    /// The device's newest snapshot seq, if it published any.
    pub floor_snapshot: Option<u64>,
    /// The chunk seqs the late joiner must fetch and replay.
    pub chunk_seqs: Vec<u64>,
}

/// The late-join floor for one device: its snapshots and chunk seqs
/// (as listed from the subtree). Order-free and total. The global
/// late join is every device's replay folded together — order-free
/// because the fold is; the snapshot-floor equivalence law
/// (`fold(snapshot ⊕ tail) == fold(all chunks)`) is proptested as P1.
#[must_use]
pub fn late_join_floor(snapshots: &BTreeSet<u64>, chunks: &BTreeSet<u64>) -> DeviceReplay {
    let floor_snapshot = snapshots.iter().next_back().copied();
    let chunk_seqs = match floor_snapshot {
        None => chunks.iter().copied().collect(),
        Some(floor) => chunks.iter().copied().filter(|seq| *seq > floor).collect(),
    };
    DeviceReplay {
        floor_snapshot,
        chunk_seqs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_payload::{EntityEntry, Field, FieldValue};
    use crate::board_snapshot::BoardSnapshot;
    use chrono::DateTime;
    use proptest::collection;
    use proptest::prelude::*;
    use serde_json::json;
    use taskboard_domain::test_support::proptest_config;
    use uuid::Uuid;

    #[test]
    fn the_constants_are_pinned() {
        assert_eq!(SNAPSHOT_EVERY_N_CHUNKS, 16);
        assert_eq!(BOARD_CHUNK_FORMAT, 1);
    }

    #[test]
    fn snapshot_due_truth_table() {
        assert!(snapshot_due(0, true), "boot always snapshots");
        assert!(snapshot_due(SNAPSHOT_EVERY_N_CHUNKS, false));
        assert!(snapshot_due(u64::MAX, false));
        assert!(!snapshot_due(0, false));
        assert!(!snapshot_due(SNAPSHOT_EVERY_N_CHUNKS - 1, false));
    }

    #[test]
    fn next_chunk_range_is_monotonic_and_gap_free() {
        assert_eq!(next_chunk_range(None, 3), Some((1, 3)));
        assert_eq!(next_chunk_range(Some(16), 1), Some((17, 17)));
        let mut last = None;
        for entries in [2, 5, 1] {
            let (first, range_last) = next_chunk_range(last, entries).expect("nonempty");
            assert_eq!(first, last.map_or(1, |l| l + 1), "gap-free");
            assert_eq!(range_last - first + 1, entries as u64);
            last = Some(range_last);
        }
    }

    /// The empty-chunk law: zero entries never publishes.
    #[test]
    fn a_zero_entry_chunk_is_unrepresentable() {
        assert_eq!(next_chunk_range(None, 0), None);
        assert_eq!(next_chunk_range(Some(141), 0), None);
    }

    #[test]
    fn gc_eligible_owner_and_watermark_pins() {
        let own = BTreeSet::from([1, 2, 3, 4, 5]);
        // Leg one: no published watermark — nothing is deletable.
        assert_eq!(gc_eligible(&own, None), [] as [u64; 0]);
        // Leg two: the watermark bounds — only ≤ the snapshot's seq.
        assert_eq!(gc_eligible(&own, Some(3)), vec![1, 2, 3]);
        assert_eq!(gc_eligible(&own, Some(0)), Vec::<u64>::new());
        assert_eq!(gc_eligible(&own, Some(99)), vec![1, 2, 3, 4, 5]);
        // Owner-scoped by API shape: the input *is* the owner's own
        // seq set; a foreign device's seqs are a different device's
        // call with its own set — there is no path that mixes them.
        let foreign = BTreeSet::from([7, 8]);
        assert_eq!(gc_eligible(&foreign, Some(3)), Vec::<u64>::new());
    }

    #[test]
    fn late_join_floor_picks_the_newest_snapshot_and_skips_subsumed_chunks() {
        let snapshots = BTreeSet::from([8, 16, 4]);
        let chunks = BTreeSet::from([1, 4, 8, 9, 10, 17]);
        let replay = late_join_floor(&snapshots, &chunks);
        assert_eq!(replay.floor_snapshot, Some(16));
        assert_eq!(
            replay.chunk_seqs,
            vec![17],
            "only chunks past the floor 16 replay"
        );
    }

    #[test]
    fn a_device_without_snapshots_replays_everything() {
        let replay = late_join_floor(&BTreeSet::new(), &BTreeSet::from([3, 1, 2]));
        assert_eq!(replay.floor_snapshot, None);
        assert_eq!(replay.chunk_seqs.len(), 3);
        assert_eq!(
            replay,
            DeviceReplay {
                floor_snapshot: None,
                chunk_seqs: vec![1, 2, 3],
            }
        );
    }

    // ---- The lifecycle laws (P1, P2, and the end-to-end tie) ----------
    //
    // A generated board session: devices write chunks of entity fields
    // with globally distinct clocks; devices snapshot at chosen seqs;
    // GC runs at the published watermark; a late joiner replays
    // snapshot ⊕ tail. Every law below would fail on a wrong floor, a
    // watermark past the snapshot, or a seq-ordered fold.

    const ENTITY_IDS: [u128; 2] = [0xA1, 0xB2];

    /// One write of one field of one entity, as a single-slot
    /// materialized entity (the unit a chunk carries).
    fn write(id: u128, field: &str, value: serde_json::Value, clock: u64) -> EntityEntry {
        EntityEntry::Modeled(crate::board_payload::MaterializedEntity {
            kind: crate::board_payload::EntityKind::Task,
            id: Uuid::from_u128(id),
            fields: std::collections::BTreeMap::from([(
                field.to_owned(),
                Field::Value(FieldValue {
                    value,
                    clock: DateTime::from_timestamp(clock.cast_signed(), 0).expect("clock"),
                    by: None,
                }),
            )]),
            memberships: None,
            unknown: std::collections::BTreeMap::new(),
        })
    }

    /// A generated session: per device, chunks 1..=n each carrying
    /// 1..=3 writes with strictly increasing global clocks, and a
    /// snapshot at a chosen chunk seq. Clocks are strictly increasing
    /// with (device, chunk) so every chunk carries at least one write
    /// newer than everything before it — precisely the shape where a
    /// wrong floor or an over-bounded watermark changes the fold.
    #[derive(Debug, Clone)]
    struct Session {
        /// Per device: (chunk seq, writes).
        devices: Vec<Vec<(u64, Vec<EntityEntry>)>>,
        /// Per device: the snapshot seq (if any).
        snapshots: Vec<Option<u64>>,
    }

    fn session_strategy() -> impl Strategy<Value = Session> {
        (1usize..=3usize, collection::vec(1usize..=4usize, 1..=3)).prop_flat_map(
            |(device_count, chunk_counts)| {
                (
                    Just(chunk_counts.clone()),
                    collection::vec(proptest::option::of(1u64..=4u64), device_count),
                )
                    .prop_map(move |(chunk_counts, snapshot_picks)| {
                        let mut clock = 1_000u64;
                        let mut devices = Vec::new();
                        let mut snapshots = Vec::new();
                        for (device, chunk_count) in chunk_counts.into_iter().enumerate() {
                            let mut chunks = Vec::new();
                            for seq in 1..=chunk_count as u64 {
                                let writes: Vec<EntityEntry> =
                                    (0..=(usize::try_from(seq).expect("small seq") + device) % 3)
                                        .map(|i| {
                                            clock += 1;
                                            let id = ENTITY_IDS[(i + device) % ENTITY_IDS.len()];
                                            let field = if i % 4 == 3 && i > 0 {
                                                // Interleaved deletes: a tombstone
                                                // field write like any other.
                                                "deleted"
                                            } else {
                                                "title"
                                            };
                                            let value = if field == "deleted" {
                                                json!(true)
                                            } else {
                                                json!(format!("d{clock}"))
                                            };
                                            write(id, field, value, clock)
                                        })
                                        .collect();
                                chunks.push((seq, writes));
                            }
                            let snapshot =
                                snapshot_picks
                                    .get(device)
                                    .copied()
                                    .flatten()
                                    .filter(|pick| {
                                        usize::try_from(*pick).is_ok_and(|pick| pick <= chunk_count)
                                    });
                            snapshots.push(snapshot.map(|pick| pick.min(chunk_count as u64)));
                            devices.push(chunks);
                        }
                        Session { devices, snapshots }
                    })
            },
        )
    }

    fn session_writes(
        session: &Session,
        device: usize,
        max_seq: Option<u64>,
    ) -> impl Iterator<Item = &EntityEntry> {
        session.devices[device]
            .iter()
            .filter(move |(seq, _)| max_seq.is_none_or(|max| *seq <= max))
            .flat_map(|(_, writes)| writes.iter())
    }

    fn fold_entries<'a>(
        entries: impl Iterator<Item = &'a EntityEntry>,
    ) -> crate::board_fold::BoardState {
        let owned: Vec<EntityEntry> = entries.cloned().collect();
        crate::board_fold::fold_board(owned.iter())
    }

    proptest! {
        #![proptest_config(proptest_config(128))]

        /// P1 — the snapshot-floor equivalence law: for every device,
        /// fold(snapshot ⊕ chunks > floor) equals fold(all chunks);
        /// the late joiner's converged state is the incremental
        /// fold's. On a wrong floor (e.g. the floor above the true
        /// snapshot seq) a live chunk is skipped and its strictly
        /// newer write changes the fold — the property *can* fail.
        #[test]
        fn the_snapshot_floor_equals_the_full_replay(session in session_strategy()) {
            let all: Vec<EntityEntry> = (0..session.devices.len())
                .flat_map(|d| session_writes(&session, d, None).cloned().collect::<Vec<_>>())
                .collect();
            let full = fold_entries(all.iter());

            let mut replay_inputs: Vec<EntityEntry> = Vec::new();
            for (device, chunks) in session.devices.iter().enumerate() {
                let floor = session.snapshots[device];
                // The snapshot state is the owner's fold over its own
                // chunks ≤ the snapshot seq.
                if let Some(floor) = floor {
                    let own: Vec<EntityEntry> = session_writes(&session, device, Some(floor))
                        .cloned()
                        .collect();
                    let snapshot_state = fold_entries(own.iter());
                    let snapshot = BoardSnapshot {
                sections: None,
                        unknown: std::collections::BTreeMap::new(),
                        format_version: BOARD_CHUNK_FORMAT,
                        device_id: crate::ids::DeviceId::mint(),
                        seq: floor,
                        written_at: DateTime::from_timestamp(1_800_000_000, 0).expect("clock"),
                        state: crate::board_snapshot::SnapshotState {
                            unknown: std::collections::BTreeMap::new(),
                            entities: snapshot_state
                                .entities
                                .values()
                                .map(|e| EntityEntry::Modeled(e.clone()))
                                .chain(
                                    snapshot_state
                                        .verbatim
                                        .values()
                                        .map(|v| EntityEntry::Verbatim(v.clone())),
                                )
                                .collect(),
                        },
                    };
                    replay_inputs.extend(snapshot.state.entities.iter().cloned());
                }
                let tail: Vec<EntityEntry> = chunks
                    .iter()
                    .filter(|(seq, _)| floor.is_none_or(|f| *seq > f))
                    .flat_map(|(_, writes)| writes.clone())
                    .collect();
                replay_inputs.extend(tail);
            }
            let replayed = fold_entries(replay_inputs.iter());
            prop_assert_eq!(&replayed, &full);
        }

        /// The adversarial leg of P1: the generated sessions include
        /// devices whose top chunk carries the globally newest write
        /// for its entity — skipping it (a floor one past the true
        /// snapshot) must change the fold, or the property would
        /// never catch a wrong floor.
        #[test]
        fn a_wrong_floor_is_detectable(session in session_strategy()) {
            prop_assume!(session.devices.len() == 1);
            let chunk_count = session.devices[0].len() as u64;
            prop_assume!(chunk_count >= 2);
            // The single device's newest write sits in the top chunk.
            let top_write_clock = session_writes(&session, 0, Some(chunk_count))
                .map(|w| match w {
                    EntityEntry::Modeled(e) => match e.fields.values().next().expect("one slot") {
                        Field::Value(fv) => fv.clock.timestamp().cast_unsigned(),
                        Field::Unknown(_) => 0,
                    },
                    EntityEntry::Verbatim(_) => 0,
                })
                .max()
                .expect("a nonempty chunk");
            let earlier_max = session_writes(&session, 0, Some(chunk_count - 1))
                .map(|w| match w {
                    EntityEntry::Modeled(e) => match e.fields.values().next().expect("one slot") {
                        Field::Value(fv) => fv.clock.timestamp().cast_unsigned(),
                        Field::Unknown(_) => 0,
                    },
                    EntityEntry::Verbatim(_) => 0,
                })
                .max()
                .unwrap_or(0);
            prop_assume!(top_write_clock > earlier_max, "the top chunk must carry a newer write");
            let full: Vec<EntityEntry> = session_writes(&session, 0, None)
                .cloned()
                .collect();
            let without_top: Vec<EntityEntry> = session_writes(&session, 0, Some(chunk_count - 1))
                .cloned()
                .collect();
            let full_state = fold_entries(full.iter());
            let wrong_floor_state = fold_entries(without_top.iter());
            prop_assert_ne!(&wrong_floor_state, &full_state,
                "a floor that skips the top chunk changes the fold — the property can fail");
        }

        /// P2 — GC safety: deleting `gc_eligible` from any device's
        /// fold inputs never changes the fold; every eligible chunk's
        /// content is subsumed by the published snapshot. Also pins
        /// the watermark's *upper* bound: a watermark past the last
        /// published snapshot would delete un-subsumed chunks and
        /// break the fold — the API takes only the published value.
        #[test]
        fn gc_never_removes_live_data(session in session_strategy()) {
            for (device, chunks) in session.devices.iter().enumerate() {
                // GC is per-device lane-scoped: the law binds one
                // device's lane at a time.
                let full: Vec<EntityEntry> = session_writes(&session, device, None)
                    .cloned()
                    .collect();
                let full_state = fold_entries(full.iter());
                let own_seq_set: BTreeSet<u64> =
                    chunks.iter().map(|(seq, _)| *seq).collect();
                let watermark = session.snapshots[device];
                let eligible = gc_eligible(&own_seq_set, watermark);
                // After GC the lane holds the published snapshot (the
                // owner's fold ≤ watermark) plus the un-eligible
                // chunks; the fold over that must equal the full fold.
                let mut post_gc: Vec<EntityEntry> = chunks
                    .iter()
                    .filter(|(seq, _)| !eligible.contains(seq))
                    .flat_map(|(_, writes)| writes.clone())
                    .collect();
                if let Some(watermark) = watermark {
                    let own: Vec<EntityEntry> = session_writes(&session, device, Some(watermark))
                        .cloned()
                        .collect();
                    let snapshot_state = fold_entries(own.iter());
                    post_gc.extend(
                        snapshot_state
                            .entities
                            .values()
                            .map(|e| EntityEntry::Modeled(e.clone()))
                            .chain(
                                snapshot_state
                                    .verbatim
                                    .values()
                                    .map(|v| EntityEntry::Verbatim(v.clone())),
                            ),
                    );
                }
                let post_gc_state = fold_entries(post_gc.iter());
                prop_assert_eq!(&post_gc_state, &full_state);
            }
        }

        /// The end-to-end law (P1 + P2 tied): events → chunks (via
        /// the seq allocator, snapshots per the cadence rule) → GC →
        /// late join ends in the same BoardState as the full replay.
        #[test]
        fn the_lifecycle_ends_where_the_full_replay_does(session in session_strategy()) {
            let mut replay_inputs: Vec<EntityEntry> = Vec::new();
            for chunks in &session.devices {
                // Snapshot cadence: boot + every N chunks since.
                let mut since_snapshot = 0u64;
                let mut published: Option<u64> = None;
                for (seq, writes) in chunks {
                    let seq = *seq;
                    let _ = next_chunk_range(published, writes.len()).expect("nonempty");
                    since_snapshot += 1;
                    if snapshot_due(since_snapshot, seq == chunks[0].0) {
                        let own: Vec<EntityEntry> = chunks
                            .iter()
                            .filter(|(s, _)| *s <= seq)
                            .flat_map(|(_, w)| w.clone())
                            .collect();
                        let state = fold_entries(own.iter());
                        replay_inputs.extend(
                            state
                                .entities
                                .values()
                                .map(|e| EntityEntry::Modeled(e.clone())),
                        );
                        published = Some(seq);
                        since_snapshot = 0;
                    }
                }
                // GC removes the eligible chunks; the survivors plus
                // the published snapshot are what a late joiner sees.
                let own_seq_set: BTreeSet<u64> =
                    chunks.iter().map(|(seq, _)| *seq).collect();
                let eligible = gc_eligible(&own_seq_set, published);
                for (seq, writes) in chunks {
                    if !eligible.contains(seq) {
                        replay_inputs.extend(writes.iter().cloned());
                    }
                }
            }
            let full: Vec<EntityEntry> = (0..session.devices.len())
                .flat_map(|d| session_writes(&session, d, None).cloned().collect::<Vec<_>>())
                .collect();
            prop_assert_eq!(&fold_entries(replay_inputs.iter()), &fold_entries(full.iter()));
        }
    }

    // ---- P9: the backbone §7.5 resurrection scenarios (pinned, not
    // hoped) ---------------------------------------------------------

    mod resurrection {
        use super::*;
        use crate::board_fold::fold_board;

        /// The late-returning device: it went offline months ago; a
        /// peer deleted the task since (tombstone intact in the
        /// peer's snapshot); on return the returning device folds the
        /// peer's snapshot and the delete survives — resurrection is
        /// impossible while tombstones ride snapshots (they are never
        /// GC'd).
        #[test]
        fn the_late_returner_adopts_the_peers_tombstone() {
            // The returning device's stale last-known write.
            let stale = write(0xA1, "title", json!("before leaving"), 1_000);
            // The peer: edit, then delete at a later clock, then the
            // snapshot that carries the tombstone.
            let peer_edit = write(0xA1, "title", json!("while away"), 2_000);
            let peer_delete = write(0xA1, "deleted", json!(true), 3_000);
            let snapshot_state = fold_board([peer_edit.clone(), peer_delete.clone()].iter());
            let snapshot = BoardSnapshot {
                sections: None,
                unknown: std::collections::BTreeMap::new(),
                format_version: BOARD_CHUNK_FORMAT,
                device_id: crate::ids::DeviceId::mint(),
                seq: 2,
                written_at: DateTime::from_timestamp(1_800_000_000, 0).expect("clock"),
                state: crate::board_snapshot::SnapshotState {
                    unknown: std::collections::BTreeMap::new(),
                    entities: snapshot_state
                        .entities
                        .values()
                        .map(|e| EntityEntry::Modeled(e.clone()))
                        .collect(),
                },
            };
            // GC removed the peer's chunks 1..=2 behind snapshot 2 —
            // the tombstone lives on in the snapshot.
            let own = BTreeSet::from([1, 2]);
            assert_eq!(gc_eligible(&own, Some(2)), vec![1, 2]);
            // The return: fold(stale ⊕ snapshot ⊕ peers' tails).
            let converged = fold_board([stale].iter().chain(snapshot.state.entities.iter()));
            let entity = &converged
                .entities
                .get(&(
                    crate::board_payload::EntityKind::Task,
                    Uuid::from_u128(0xA1),
                ))
                .expect("the entity survives");
            let deleted = match entity.fields["deleted"] {
                Field::Value(ref fv) => fv.value == json!(true),
                Field::Unknown(_) => false,
            };
            assert!(deleted, "the tombstone survives the late return");
        }

        /// The concurrent delete-vs-edit duel: the later clock wins
        /// deterministically in both input orders (P5 at scenario
        /// granularity).
        #[test]
        fn the_concurrent_duel_resolves_by_clock_in_both_orders() {
            let delete = write(0xA1, "deleted", json!(true), 5_000);
            let undelete = write(0xA1, "deleted", json!(false), 5_000 + 1);
            let forward = fold_board([delete.clone(), undelete.clone()].iter());
            let backward = fold_board([undelete, delete].iter());
            assert_eq!(forward, backward);
            let entity = &forward.entities[&(
                crate::board_payload::EntityKind::Task,
                Uuid::from_u128(0xA1),
            )];
            let deleted = match entity.fields["deleted"] {
                Field::Value(ref fv) => fv.value == json!(true),
                Field::Unknown(_) => false,
            };
            assert!(!deleted, "the later un-delete beats the older tombstone");
        }

        /// The folder fork: a duplicated root (both copies carry the
        /// same events) merges by set-union — the fold over both
        /// roots equals the fold over one (idempotence at root
        /// granularity), and disjoint events union.
        #[test]
        fn a_forked_folder_merges_by_set_union() {
            let root_a: Vec<EntityEntry> = vec![
                write(0xA1, "title", json!("forked"), 1_000),
                write(0xB2, "title", json!("only in a"), 1_001),
            ];
            // The fork copy: byte-identical duplicate events.
            let root_b: Vec<EntityEntry> = root_a.clone();
            let single = fold_board(root_a.iter());
            let both = fold_board(root_a.iter().chain(root_b.iter()));
            assert_eq!(both, single, "a duplicated event changes nothing");
            // Disjoint events union: the fork's unique write joins the
            // fold as its own duel participant.
            let root_c: Vec<EntityEntry> =
                vec![write(0xB2, "title", json!("only in the fork"), 1_002)];
            let unioned = fold_board(root_a.iter().chain(root_c.iter()));
            let entity = &unioned.entities[&(
                crate::board_payload::EntityKind::Task,
                Uuid::from_u128(0xB2),
            )];
            let title = match entity.fields["title"] {
                Field::Value(ref fv) => fv.value.clone(),
                Field::Unknown(_) => panic!("modeled slot"),
            };
            assert_eq!(
                title,
                json!("only in the fork"),
                "the later write wins the duel"
            );
            assert!(unioned != single, "a disjoint event changes the fold");
        }
    }
}

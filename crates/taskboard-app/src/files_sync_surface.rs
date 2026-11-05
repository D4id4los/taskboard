// SPDX-License-Identifier: AGPL-3.0-only
//! The files-sync status surface's app-side glue (DR-8 chunk 4,
//! D-FH5): the transport crate's typed findings mapped onto the UI
//! crate's plain chip model, and the watch-to-watch pump that
//! forwards it at mount. The mapping lives here because the
//! dependency fence forbids the UI crate from naming the IO crate
//! (and the orphan rule forbids a `From` impl across the two foreign
//! types) — the app sees both.

use tokio::sync::watch;
use tokio::task::JoinHandle;

use taskboard_sync_files_nextcloud::status::{CycleOutcome, FilesSyncStatus};
use taskboard_ui_slint::presentation::{FilesSyncChip, FilesSyncChipState};

/// The chip's precedence ladder (D-FH5): Forked loudest, then
/// Diverged, Parked, `CycleFailed`, `Ok` — the fork halts data flow
/// (loudest), a park is informational, a failed cycle is transient.
/// Empty/default status maps to `Ok`.
#[must_use]
pub fn files_sync_chip(status: &FilesSyncStatus) -> FilesSyncChip {
    let state = if !status.forked_paths.is_empty() {
        FilesSyncChipState::Forked
    } else if !status.diverged_from.is_empty() {
        FilesSyncChipState::Diverged
    } else if !status.parked.is_empty() {
        FilesSyncChipState::Parked
    } else if matches!(status.last_cycle, Some(CycleOutcome::Failed)) {
        FilesSyncChipState::CycleFailed
    } else {
        FilesSyncChipState::Ok
    };
    FilesSyncChip { state }
}

/// Seeds a UI-typed watch from the transport status watch and spawns
/// the forwarding task: every transport update maps onto the chip
/// watch; the pump exits when the transport actor drops its sender
/// (shutdown) — the same channel-close lifecycle posture as the other
/// runtimes. The receiver half goes into `mount` (`None` from a
/// `None` input — no chip lane at all when the substrate is off).
#[must_use]
pub fn chip_channel(
    status: Option<watch::Receiver<FilesSyncStatus>>,
) -> Option<(JoinHandle<()>, watch::Receiver<FilesSyncChip>)> {
    let mut status = status?;
    let seed = files_sync_chip(&status.borrow_and_update().clone());
    let (tx, rx) = watch::channel(seed);
    let join = tokio::spawn(async move {
        while status.changed().await.is_ok() {
            let chip = files_sync_chip(&status.borrow().clone());
            tx.send_replace(chip);
        }
    });
    Some((join, rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    use taskboard_sync_files_nextcloud::status::ParkKind;

    fn status(
        forked: bool,
        _diverged: bool,
        parked: bool,
        last_cycle: Option<CycleOutcome>,
    ) -> FilesSyncStatus {
        FilesSyncStatus {
            last_cycle,
            forked_paths: forked
                .then(|| "taskboard/devices/x/appdata.json".to_owned())
                .into_iter()
                .collect::<BTreeSet<_>>(),
            parked: parked
                .then(|| {
                    (
                        "taskboard/devices/y/info.json".to_owned(),
                        ParkKind::Quarantined,
                    )
                })
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
            ..FilesSyncStatus::default()
        }
    }

    fn diverged(mut status: FilesSyncStatus) -> FilesSyncStatus {
        status.diverged_from =
            taskboard_sync_files::ids::DeviceId::parse("0193c000-0000-7c89-ab01-2345abcdef01")
                .map(|id| BTreeSet::from([id]))
                .expect("fixture id");
        status
    }

    /// The ladder: every louder rung wins over the quieter ones below
    /// it; all-clear (and the default) is `Ok`.
    #[test]
    fn the_mapping_walks_the_precedence_ladder() {
        use FilesSyncChipState as S;
        let failed = || Some(CycleOutcome::Failed);
        let ok = || {
            Some(CycleOutcome::Ok {
                published: false,
                read_devices: 0,
                import_rejected: 0,
            })
        };
        // Each rung alone.
        assert_eq!(
            files_sync_chip(&status(true, false, false, failed())).state,
            S::Forked
        );
        assert_eq!(
            files_sync_chip(&diverged(status(false, true, false, failed()))).state,
            S::Diverged
        );
        assert_eq!(
            files_sync_chip(&status(false, false, true, failed())).state,
            S::Parked
        );
        assert_eq!(
            files_sync_chip(&status(false, false, false, failed())).state,
            S::CycleFailed
        );
        assert_eq!(
            files_sync_chip(&status(false, false, false, ok())).state,
            S::Ok
        );
        // The fork is loudest even when everything else coexists.
        let everything = diverged(status(true, true, true, failed()));
        assert_eq!(files_sync_chip(&everything).state, S::Forked);
        // Empty and default are `Ok`.
        assert_eq!(files_sync_chip(&FilesSyncStatus::default()).state, S::Ok);
    }

    /// The pump journey: three transport statuses map through in
    /// order; dropping the transport sender ends the pump (the join
    /// resolves, the chip watch closes).
    #[tokio::test]
    async fn the_pump_forwards_mapped_chips_and_exits_on_sender_drop() {
        use FilesSyncChipState as S;
        let (tx, rx) = watch::channel(FilesSyncStatus::default());
        let (join, chips) = chip_channel(Some(rx)).expect("the lane exists");
        let mut chips = chips;
        assert_eq!(
            chips.borrow().state,
            S::Ok,
            "seeded from the current borrow"
        );
        for (status, expected) in [
            (
                status(false, false, false, Some(CycleOutcome::Failed)),
                S::CycleFailed,
            ),
            (status(false, false, true, None), S::Parked),
            (
                status(true, true, true, Some(CycleOutcome::Failed)),
                S::Forked,
            ),
        ] {
            tx.send(status).expect("watch send");
            chips.changed().await.expect("the pump forwards");
            assert_eq!(chips.borrow().state, expected);
        }
        drop(tx);
        // The pump's exit: changed() errs once the sender half dies and
        // the pump returns — the join resolving is the lifecycle pin.
        chips.changed().await.expect_err("the chip watch closes");
        join.await.expect("the pump ends");
    }

    /// The off arm: no transport watch, no chip lane — `None` in, and
    /// nothing spawned.
    #[tokio::test]
    async fn no_transport_watch_means_no_chip_lane() {
        assert!(chip_channel(None).is_none());
    }
}

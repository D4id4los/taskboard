// SPDX-License-Identifier: MIT OR Apache-2.0
//! The engine's handle-side activity counters (#145).
//!
//! [`EngineMetrics`] lives on the [`EngineHandle`](crate::EngineHandle),
//! not in [`AppState`]: the counters are engine-activity facts, not
//! application state — publishing them would churn every snapshot for
//! numbers only a diagnostics surface reads. Relaxed ordering suffices
//! (per-counter independence is fine: observability, no synchronization
//! role); monotonicity comes from `fetch_add`, wrap risk is the u64
//! horizon.

use std::sync::atomic::{AtomicU64, Ordering};

/// The running engine's atomic counters: one `Arc` clone held on the
/// handle and one inside the loop (threaded through the injected
/// `EngineParts`). Minted fresh per engine by `spawn_state_engine` —
/// counters restart at zero on every boot.
#[derive(Debug, Default)]
pub struct EngineMetrics {
    /// Mutating commands accepted past the drain gate (`EngineEnvelope::
    /// Execute` only — flushes, state reads, and connector attaches are
    /// plumbing, not commands; declined commands are refusals, not work).
    commands: AtomicU64,
    /// Semantically rejected commands (the typed `Rejected` receipt).
    rejections: AtomicU64,
    /// `StateUpdated` sends attempted by `publish` — every engine-driven
    /// repaint trigger, including zero-receiver sends: the honest
    /// engine-activity number.
    signals: AtomicU64,
}

/// A consistent-enough point-in-time read of [`EngineMetrics`] (each
/// counter loads independently — the snapshot never feeds control flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineMetricsSnapshot {
    /// Accepted mutating commands.
    pub commands: u64,
    /// Semantically rejected commands.
    pub rejections: u64,
    /// Publish/repaint signals sent (attempted sends included).
    pub signals: u64,
}

impl EngineMetrics {
    /// Increments the accepted-command counter (Relaxed).
    pub fn count_command(&self) {
        let _ = self.commands.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the rejection counter (Relaxed).
    pub fn count_rejection(&self) {
        let _ = self.rejections.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the publish-signal counter (Relaxed).
    pub fn count_signal(&self) {
        let _ = self.signals.fetch_add(1, Ordering::Relaxed);
    }

    /// Point-in-time read into a copyable snapshot (Relaxed loads).
    #[must_use]
    pub fn snapshot(&self) -> EngineMetricsSnapshot {
        EngineMetricsSnapshot {
            commands: self.commands.load(Ordering::Relaxed),
            rejections: self.rejections.load(Ordering::Relaxed),
            signals: self.signals.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_start_zeroed_and_count_independently() {
        let metrics = EngineMetrics::default();
        assert_eq!(
            metrics.snapshot(),
            EngineMetricsSnapshot {
                commands: 0,
                rejections: 0,
                signals: 0,
            }
        );
        metrics.count_command();
        metrics.count_command();
        metrics.count_signal();
        assert_eq!(
            metrics.snapshot(),
            EngineMetricsSnapshot {
                commands: 2,
                rejections: 0,
                signals: 1,
            }
        );
    }

    #[test]
    fn the_snapshot_is_copy_and_value_comparable() {
        let metrics = EngineMetrics::default();
        metrics.count_rejection();
        let first = metrics.snapshot();
        let second = first;
        assert_eq!(first, second);
        assert_eq!(second.rejections, 1);
    }
}

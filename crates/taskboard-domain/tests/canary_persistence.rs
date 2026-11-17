// SPDX-License-Identifier: MIT OR Apache-2.0
//! Canary for the #857 persistence-mechanism evidence capture — DELIBERATELY
//! FAILING and never intended to merge. A failing proptest writes a
//! counterexample into `proptest-regressions/`, which the CI failure
//! artifact upload must carry out of the job. Removed after capture.

use proptest::prelude::*;

proptest! {
    // A deterministic always-failing property: the counterexample is the
    // minimal input proptest finds on the first run, persisted to
    // proptest-regressions/canary.txt by the default failure persistence.
    #[test]
    fn canary_always_fails_to_persist_a_counterexample(x in 0u32..1000) {
        prop_assert!(x >= 1000, "canary: deliberate failure, input={x}");
    }
}

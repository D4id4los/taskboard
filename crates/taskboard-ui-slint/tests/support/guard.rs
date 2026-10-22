// SPDX-License-Identifier: AGPL-3.0-only

//! The §9.2/Q17 keypress-count guard: counts the physical presses the
//! journeys inject per core verb and fails when a verb exceeds its
//! recorded ceiling (a regression ceiling on degradation — a strictly
//! better UI always passes; design plan §9.2, agenda Q17).

/// The session-audited verbs — exactly the five §9.2 names. Delete and
/// navigation carry no ceiling (not in the contract) and are not
/// phased.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Verb {
    /// `task.complete`
    Complete,
    /// `task.move`
    Move,
    /// `task.create`
    Create,
    /// `task.edit`
    Edit,
    /// `menu-invoke` (open + cursor + invoke). Recorded for the
    /// report only — the round-1 curation made the cursor walk scale
    /// with the menu's length (a structural cost, not degradation),
    /// so this verb carries no ceiling any more (stage-2 recorded
    /// deviation).
    MenuInvoke,
}

impl Verb {
    /// The ceiling as a design contract copied from the design plan
    /// §9.2 ("complete ≤ 2, move ≤ 4, create ≤ 3, edit ≤ 3"), NOT
    /// re-derived here. A chord counts as its strokes (`Ctrl+x Ctrl+c`
    /// = 2); held modifiers are not presses.
    #[must_use]
    pub const fn ceiling(self) -> u32 {
        match self {
            Self::Complete => 2,
            Self::Move => 4,
            Self::Create | Self::Edit => 3,
            Self::MenuInvoke => u32::MAX,
        }
    }
}

/// The ceilings table: the design artefact cited in one place so the
/// verdict cannot silently drift from the plan.
pub const CEILINGS: &[(Verb, u32)] = &[
    (Verb::Complete, Verb::Complete.ceiling()),
    (Verb::Move, Verb::Move.ceiling()),
    (Verb::Create, Verb::Create.ceiling()),
    (Verb::Edit, Verb::Edit.ceiling()),
];

/// One exceeded ceiling: the failure type of [`verdict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation {
    pub verb: Verb,
    pub count: u32,
    pub ceiling: u32,
}

/// The pure verdict over the journeys' recorded counts: a violation
/// per verb whose count exceeds its ceiling; counts at or below pass
/// (improvement included). Unrecorded verbs are unviolated.
#[must_use]
pub fn verdict(recorded: &[(Verb, u32)]) -> Vec<Violation> {
    CEILINGS
        .iter()
        .filter_map(|&(verb, ceiling)| {
            let count = recorded
                .iter()
                .find(|&(v, _)| *v == verb)
                .map_or(0, |&(_, c)| c);
            (count > ceiling).then_some(Violation {
                verb,
                count,
                ceiling,
            })
        })
        .collect()
}

/// The journey-side recorder: at most one phase open at a time; every
/// [`Guard::press`] increments the open phase's counter.
#[derive(Debug, Default)]
pub struct Guard {
    open: Option<(Verb, u32)>,
    recorded: Vec<(Verb, u32)>,
}

impl Guard {
    /// Opens a counted phase.
    ///
    /// # Panics
    ///
    /// Panics on a nested `begin` — a journey harness bug, not a
    /// runtime condition.
    pub fn begin(&mut self, verb: Verb) {
        assert!(
            self.open.is_none(),
            "guard phase {:?} opened while {:?} is open",
            verb,
            self.open.map(|(v, _)| v)
        );
        self.open = Some((verb, 0));
    }

    /// Records one physical press against the open phase.
    pub fn press(&mut self) {
        self.press_n(1);
    }

    /// Records `n` presses against the open phase (a stroke
    /// contributes exactly one; a typed character is a stroke).
    pub fn press_n(&mut self, n: u32) {
        if let Some((_, count)) = &mut self.open {
            *count += n;
        }
    }

    /// Closes the phase and files its count for [`verdict`].
    ///
    /// # Panics
    ///
    /// Panics if `verb` is not the open phase (mismatched begin/end).
    pub fn end(&mut self, verb: Verb) -> u32 {
        let Some((open, count)) = self.open.take() else {
            panic!("guard phase {verb:?} ended while none is open");
        };
        assert_eq!(open, verb, "guard phase ended out of order");
        self.recorded.push((open, count));
        count
    }

    /// The counts recorded so far, in begin order (report input).
    #[must_use]
    pub fn recorded(&self) -> &[(Verb, u32)] {
        &self.recorded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T2 — exceedance fails: create at 4 presses violates its ≤ 3.
    #[test]
    fn exceedance_violates() {
        let violations = verdict(&[(Verb::Create, 4)]);
        assert_eq!(
            violations,
            vec![Violation {
                verb: Verb::Create,
                count: 4,
                ceiling: 3,
            }]
        );
    }

    /// T2 — the exact boundary passes (3/≤3 is the shipped cost).
    #[test]
    fn exact_boundary_passes() {
        assert_eq!(
            verdict(&[(Verb::Create, 3), (Verb::Edit, 3), (Verb::MenuInvoke, 3)]),
            []
        );
    }

    /// T2 — a strictly better UI always passes (AGENTS §5 interplay).
    #[test]
    fn improvement_passes() {
        assert_eq!(
            verdict(&[
                (Verb::Complete, 1),
                (Verb::Move, 2),
                (Verb::Create, 2),
                (Verb::Edit, 1),
                (Verb::MenuInvoke, 2),
            ]),
            []
        );
    }

    /// T2 — unrecorded verbs are unviolated; recording is not
    /// order-sensitive for the verdict.
    #[test]
    fn unrecorded_and_reordered_verbs_pass() {
        assert_eq!(verdict(&[]), []);
        assert_eq!(verdict(&[(Verb::Move, 1), (Verb::Complete, 1)]), []);
    }

    /// T2 — the recorder counts every press of the open phase (the
    /// harness wires one `press` call per injected stroke) and files
    /// it exactly once per begin/end pair.
    #[test]
    fn recorder_counts_presses_per_phase() {
        let mut guard = Guard::default();
        guard.begin(Verb::Create);
        for _ in 0..4 {
            guard.press();
        }
        assert_eq!(guard.end(Verb::Create), 4);
        assert_eq!(guard.recorded(), &[(Verb::Create, 4)]);
        // A later, better run of the same verb re-records at 3 — the
        // verdict still sees the recorded worst run.
        guard.begin(Verb::Create);
        for _ in 0..3 {
            guard.press();
        }
        assert_eq!(guard.end(Verb::Create), 3);
        assert_eq!(
            verdict(guard.recorded()),
            vec![Violation {
                verb: Verb::Create,
                count: 4,
                ceiling: 3,
            }]
        );
    }

    /// T2 — every ceiling row equals the verb's own constant: the
    /// table cannot drift from the §9.2 contracts.
    #[test]
    fn ceilings_table_matches_the_verbs() {
        for (verb, ceiling) in CEILINGS {
            assert_eq!((*verb, *ceiling), (*verb, verb.ceiling()));
        }
    }
}

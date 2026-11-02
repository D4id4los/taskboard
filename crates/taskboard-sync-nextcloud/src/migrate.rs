// SPDX-License-Identifier: MIT OR Apache-2.0
//! The cross-board sidecar repair (#350): the sibling-document lookup
//! and the migration-delta predicate (plan chunk 4.3, D1–D13).
//!
//! When a card moves between two boards this connector owns, the origin
//! board's document stops being consulted while the destination board
//! adopts the card with no document entry — the sidecar-carried fields
//! die with the move. This module repairs the transport lane: for every
//! card in the destination snapshot, look the card id up across
//! **sibling-board** sources (the last ingested document per board, and
//! the GC-eviction reservoir), and produce a *migration fragment* —
//! structural clones of the origin entries — that the flush unions into
//! the destination document.
//!
//! Soundness of the bare-card-id lookup key: Deck card ids come from one
//! server-wide sequence (deck-roundtrip report §3.2 — the attachment
//! layout is keyed by card id alone), so a card id identifies the same
//! card on every board of one connector. Cross-server moves have no
//! sibling to consult and are never repaired (partial by construction).
//!
//! Failure isolation is total by construction: every operation here is
//! an in-memory map lookup — infallible — so no lookup failure can
//! block or abort the adoption path. A missing sibling (no document
//! ingested, or its GC already dropped the entry) collapses to "the
//! fragment contributes nothing for that card": adoption proceeds
//! without the entry, exactly the pre-repair behavior.

use std::collections::{BTreeMap, BTreeSet};

use taskboard_domain::{
    AppDataDocument, AppDataSections, DismissalsEntry, RemoteBoardId, RuleEntry, RulesEntry,
    TaskEntry,
};

/// One sibling board's migration sources: the live document cache slot
/// (the last successful ingest — cache-first per D3) and the GC-eviction
/// reservoir (D11 — consulted only where the cache lacks the card).
pub(crate) struct SiblingView<'a> {
    pub document: Option<&'a AppDataDocument>,
    pub reservoir: &'a AppDataSections,
}

/// `true` when the fragment carries no section at all (the flush work
/// gate's third clause reads exactly this).
pub(crate) fn is_empty(sections: &AppDataSections) -> bool {
    sections.tasks.is_none() && sections.reminder_rules.is_none() && sections.dismissals.is_none()
}

/// The migration fragment: for every card the destination snapshot
/// lists (`present_cards`), the sibling entries whose content is new to
/// the destination `base` (D9's additive-strictly-newer predicate),
/// cloned structurally — clocks unchanged (D8). `siblings` is iterated
/// in its own (board-id) order; the first sibling contributing a card
/// wins it whole (a card lives on exactly one board, so cross-sibling
/// mixing has no legitimate shape).
pub(crate) fn migrate_fragment(
    base: &AppDataSections,
    present_cards: &BTreeSet<String>,
    siblings: &BTreeMap<RemoteBoardId, SiblingView<'_>>,
) -> AppDataSections {
    let mut tasks: BTreeMap<String, TaskEntry> = BTreeMap::new();
    let mut rules: BTreeMap<String, RulesEntry> = BTreeMap::new();
    let mut dismissals: BTreeMap<String, DismissalsEntry> = BTreeMap::new();
    for card in present_cards {
        for view in siblings.values() {
            // Cache first; the reservoir shadows only where the cache
            // lacks the card (D11's pinned order, T8).
            let sibling_tasks = view
                .document
                .and_then(|document| document.sections.tasks.as_ref())
                .and_then(|entries| entries.get(card))
                .or_else(|| {
                    view.reservoir
                        .tasks
                        .as_ref()
                        .and_then(|entries| entries.get(card))
                });
            let sibling_rules = view
                .document
                .and_then(|document| document.sections.reminder_rules.as_ref())
                .and_then(|entries| entries.get(card))
                .or_else(|| {
                    view.reservoir
                        .reminder_rules
                        .as_ref()
                        .and_then(|entries| entries.get(card))
                });
            let sibling_dismissals = view
                .document
                .and_then(|document| document.sections.dismissals.as_ref())
                .and_then(|entries| entries.get(card))
                .or_else(|| {
                    view.reservoir
                        .dismissals
                        .as_ref()
                        .and_then(|entries| entries.get(card))
                });
            let mut contributed = false;
            if let Some(entry) = sibling_tasks
                && task_entry_triggers(
                    base.tasks.as_ref().and_then(|entries| entries.get(card)),
                    entry,
                )
            {
                tasks.insert(card.clone(), entry.clone());
                contributed = true;
            }
            if let Some(entry) = sibling_rules
                && rules_entry_triggers(
                    base.reminder_rules
                        .as_ref()
                        .and_then(|entries| entries.get(card)),
                    entry,
                )
            {
                rules.insert(card.clone(), entry.clone());
                contributed = true;
            }
            if let Some(entry) = sibling_dismissals
                && dismissals_entry_triggers(
                    base.dismissals
                        .as_ref()
                        .and_then(|entries| entries.get(card)),
                    entry,
                )
            {
                dismissals.insert(card.clone(), entry.clone());
                contributed = true;
            }
            if contributed {
                break;
            }
        }
    }
    AppDataSections {
        tasks: (!tasks.is_empty()).then_some(tasks),
        reminder_rules: (!rules.is_empty()).then_some(rules),
        dismissals: (!dismissals.is_empty()).then_some(dismissals),
        ..AppDataSections::default()
    }
}

/// D9's task predicate: a base-absent card triggers; otherwise any
/// modeled field strictly newer than the base's, or any unmodeled key
/// the base entry lacks (additive content has no clocks — presence is
/// the comparison). Equal clocks never trigger: the post-migration echo
/// holds equal clocks, and triggering on equality would re-fire every
/// cycle forever — exactly the non-idempotence the repair forbids.
fn task_entry_triggers(base: Option<&TaskEntry>, sibling: &TaskEntry) -> bool {
    let Some(base) = base else {
        return true;
    };
    sibling.fields.iter().any(|(key, write)| {
        base.fields
            .get(key)
            .is_none_or(|base_write| write.clock > base_write.clock)
    }) || sibling
        .unmodeled
        .keys()
        .any(|key| !base.unmodeled.contains_key(key))
}

/// D9's rules predicate, per row: a clocked sibling row triggers when
/// the base lacks the key or is a strictly older clocked row; a
/// verbatim base row suppresses (the union never displaces verbatim
/// content — matching `union_rules_entry`); a verbatim sibling row
/// never triggers at all (it can never displace anything). The entry
/// triggers when the base lacks the card or any row triggers.
fn rules_entry_triggers(base: Option<&RulesEntry>, sibling: &RulesEntry) -> bool {
    let Some(base) = base else {
        // A verbatim-only entry contributes nothing a future union could
        // order; keep the trigger clocked-only.
        return sibling
            .rules
            .values()
            .any(|row| !matches!(row, RuleEntry::Verbatim(_)));
    };
    sibling
        .rules
        .iter()
        .any(|(key, row)| match base.rules.get(key) {
            None => !matches!(row, RuleEntry::Verbatim(_)),
            Some(base_row) => match (base_row, row) {
                // Verbatim content is never displaced and never displaces.
                (RuleEntry::Verbatim(_), _) | (_, RuleEntry::Verbatim(_)) => false,
                _ => match (base_row.clock(), row.clock()) {
                    (Some(base_clock), Some(sibling_clock)) => sibling_clock > base_clock,
                    _ => false,
                },
            },
        })
}

/// D9's dismissals predicate: append-only set union — the entry
/// triggers when the base lacks the card or any sibling row key the
/// base lacks; present rows never retrigger.
fn dismissals_entry_triggers(base: Option<&DismissalsEntry>, sibling: &DismissalsEntry) -> bool {
    let Some(base) = base else {
        return true;
    };
    sibling
        .dismissals
        .keys()
        .any(|key| !base.dismissals.contains_key(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use taskboard_domain::{
        DismissalEntry, Insistence, ModeledValue, ReminderOffset, RuleEntry, TaskFieldWrite,
    };

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn write(value: ModeledValue, clock_secs: i64) -> TaskFieldWrite {
        TaskFieldWrite {
            value,
            clock: ts(clock_secs),
        }
    }

    fn task_entry(fields: &[(&str, TaskFieldWrite)]) -> TaskEntry {
        TaskEntry {
            fields: fields
                .iter()
                .map(|(key, write)| ((*key).to_owned(), write.clone()))
                .collect(),
            unmodeled: BTreeMap::new(),
        }
    }

    fn rule_entry(clock_secs: i64) -> RuleEntry {
        RuleEntry::Rule {
            offset: ReminderOffset::at_due(),
            created: ts(100),
            clock: ts(clock_secs),
            insistence: Insistence::Normal,
        }
    }

    fn verbatim() -> RuleEntry {
        RuleEntry::Verbatim(serde_json::json!({ "mystery": true }))
    }

    fn sections_with_tasks(entries: BTreeMap<String, TaskEntry>) -> AppDataSections {
        AppDataSections {
            tasks: Some(entries),
            ..AppDataSections::default()
        }
    }

    fn sections_with_rules(entries: BTreeMap<String, RulesEntry>) -> AppDataSections {
        AppDataSections {
            reminder_rules: Some(entries),
            ..AppDataSections::default()
        }
    }

    fn sections_with_dismissals(entries: BTreeMap<String, DismissalsEntry>) -> AppDataSections {
        AppDataSections {
            dismissals: Some(entries),
            ..AppDataSections::default()
        }
    }

    fn present(cards: &[&str]) -> BTreeSet<String> {
        cards.iter().map(|card| (*card).to_owned()).collect()
    }

    fn one_sibling<'a>(
        document: Option<&'a AppDataDocument>,
        reservoir: &'a AppDataSections,
    ) -> BTreeMap<RemoteBoardId, SiblingView<'a>> {
        BTreeMap::from([(
            RemoteBoardId(1),
            SiblingView {
                document,
                reservoir,
            },
        )])
    }

    fn document_with(sections: AppDataSections) -> AppDataDocument {
        AppDataDocument {
            format: taskboard_domain::APPDATA_FORMAT,
            board: RemoteBoardId(1),
            sections,
            unknown: BTreeMap::new(),
        }
    }

    // T2: the fragment carries whole structural clones of the origin's
    // entries for cards the destination base lacks — all three sections,
    // unmodeled content and rule tombstones included.
    #[test]
    fn the_fragment_carries_entries_whose_card_the_destination_lacks() {
        let mut moved_tasks = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                500,
            ),
        )]);
        moved_tasks
            .unmodeled
            .insert("note".to_owned(), serde_json::json!("verbatim"));
        let mut moved_rules = RulesEntry::default();
        moved_rules.rules.insert(
            "00000000-0000-0000-0000-00000000000a".to_owned(),
            rule_entry(600),
        );
        moved_rules.rules.insert(
            "00000000-0000-0000-0000-00000000000b".to_owned(),
            RuleEntry::Tombstone { clock: ts(700) },
        );
        let mut moved_dismissals = DismissalsEntry::default();
        moved_dismissals.dismissals.insert(
            "00000000-0000-0000-0000-00000000000a|2026-01-01T08:00:00Z".to_owned(),
            DismissalEntry::Dismissed {
                dismissed_at: ts(800),
            },
        );
        let sibling = document_with(AppDataSections {
            tasks: Some(BTreeMap::from([("500".to_owned(), moved_tasks)])),
            reminder_rules: Some(BTreeMap::from([("500".to_owned(), moved_rules)])),
            dismissals: Some(BTreeMap::from([("500".to_owned(), moved_dismissals)])),
            ..AppDataSections::default()
        });
        let fragment = migrate_fragment(
            &AppDataSections::default(),
            &present(&["500"]),
            &one_sibling(Some(&sibling), &AppDataSections::default()),
        );
        let tasks = fragment.tasks.as_ref().unwrap();
        assert_eq!(
            tasks["500"].fields["effort"].clock,
            ts(500),
            "clock unchanged"
        );
        assert_eq!(
            tasks["500"].unmodeled["note"],
            serde_json::json!("verbatim"),
            "unmodeled content rides verbatim"
        );
        let rules = &fragment.reminder_rules.as_ref().unwrap()["500"];
        assert!(
            matches!(rules.rules["00000000-0000-0000-0000-00000000000b"], RuleEntry::Tombstone { clock } if clock == ts(700))
        );
        let dismissals = &fragment.dismissals.as_ref().unwrap()["500"];
        assert!(matches!(
            dismissals.dismissals["00000000-0000-0000-0000-00000000000a|2026-01-01T08:00:00Z"],
            DismissalEntry::Dismissed { dismissed_at } if dismissed_at == ts(800)
        ));
    }

    // T3: only strictly newer task fields trigger — equal clock excluded
    // (the idempotence edge), older excluded, one newer field includes
    // the entry, a base-absent modeled key and a base-absent unmodeled
    // key both trigger.
    #[test]
    fn only_strictly_newer_task_fields_trigger() {
        let base_entry = task_entry(&[
            (
                "effort",
                write(
                    ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                    500,
                ),
            ),
            ("importance", write(ModeledValue::Importance(None), 500)),
        ]);
        let base = sections_with_tasks(BTreeMap::from([("500".to_owned(), base_entry)]));

        // Equal clocks: never triggers (the echo's shape).
        let equal = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                500,
            ),
        )]);
        let fragment = migrate_fragment(
            &base,
            &present(&["500"]),
            &one_sibling(
                None,
                &sections_with_tasks(BTreeMap::from([("500".to_owned(), equal)])),
            ),
        );
        assert!(is_empty(&fragment), "equal clocks never re-migrate");

        // Older: no trigger either.
        let older = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(1))),
                400,
            ),
        )]);
        let fragment = migrate_fragment(
            &base,
            &present(&["500"]),
            &one_sibling(
                None,
                &sections_with_tasks(BTreeMap::from([("500".to_owned(), older)])),
            ),
        );
        assert!(is_empty(&fragment));

        // One strictly newer field anywhere in the entry: the entry rides.
        let newer = task_entry(&[
            (
                "effort",
                write(
                    ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                    500,
                ),
            ),
            ("urgency", write(ModeledValue::Urgency(None), 600)),
        ]);
        let fragment = migrate_fragment(
            &base,
            &present(&["500"]),
            &one_sibling(
                None,
                &sections_with_tasks(BTreeMap::from([("500".to_owned(), newer.clone())])),
            ),
        );
        assert_eq!(fragment.tasks.as_ref().unwrap()["500"], newer);

        // A base-absent unmodeled key triggers (additive content has no
        // clocks — presence is the comparison).
        let mut with_note = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                500,
            ),
        )]);
        with_note
            .unmodeled
            .insert("note".to_owned(), serde_json::json!("x"));
        let fragment = migrate_fragment(
            &base,
            &present(&["500"]),
            &one_sibling(
                None,
                &sections_with_tasks(BTreeMap::from([("500".to_owned(), with_note)])),
            ),
        );
        assert!(fragment.tasks.is_some(), "a missing unmodeled key triggers");
    }

    // T6's twin lives in T3's last arm; the dedicated pin: an unmodeled
    // entry migrates when the destination lacks the key.
    #[test]
    fn unmodeled_entry_content_migrates_when_the_destination_lacks_the_key() {
        let mut sibling_entry = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                500,
            ),
        )]);
        sibling_entry
            .unmodeled
            .insert("custom".to_owned(), serde_json::json!({"deep": [1, 2]}));
        let base = sections_with_tasks(BTreeMap::from([(
            "500".to_owned(),
            task_entry(&[(
                "effort",
                write(
                    ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                    500,
                ),
            )]),
        )]));
        let fragment = migrate_fragment(
            &base,
            &present(&["500"]),
            &one_sibling(
                None,
                &sections_with_tasks(BTreeMap::from([("500".to_owned(), sibling_entry.clone())])),
            ),
        );
        assert_eq!(
            fragment.tasks.as_ref().unwrap()["500"].unmodeled["custom"],
            serde_json::json!({"deep": [1, 2]})
        );
    }

    // T4: rule rows — clocked > clocked triggers; a verbatim base row
    // suppresses a clocked sibling; a verbatim sibling row never
    // triggers; a missing rule key triggers.
    #[test]
    fn only_strictly_newer_rule_rows_trigger_and_verbatim_never_displaces() {
        let base_row = (
            "00000000-0000-0000-0000-00000000000a".to_owned(),
            rule_entry(500),
        );
        let base = sections_with_rules(BTreeMap::from([(
            "500".to_owned(),
            RulesEntry {
                rules: BTreeMap::from([base_row.clone()]),
                unknown: BTreeMap::new(),
            },
        )]));
        let fragment_for = |sibling_row: RuleEntry| {
            let sibling = sections_with_rules(BTreeMap::from([(
                "500".to_owned(),
                RulesEntry {
                    rules: BTreeMap::from([(
                        "00000000-0000-0000-0000-00000000000a".to_owned(),
                        sibling_row,
                    )]),
                    unknown: BTreeMap::new(),
                },
            )]));
            migrate_fragment(&base, &present(&["500"]), &one_sibling(None, &sibling))
        };
        assert!(
            is_empty(&fragment_for(rule_entry(500))),
            "equal clocks never trigger"
        );
        assert!(
            is_empty(&fragment_for(rule_entry(400))),
            "older clocks never trigger"
        );
        assert!(
            fragment_for(rule_entry(600)).reminder_rules.is_some(),
            "strictly newer triggers"
        );
        // A verbatim base row suppresses the clocked sibling (the union
        // keeps verbatim content — triggering would re-fire forever).
        let verbatim_base = sections_with_rules(BTreeMap::from([(
            "500".to_owned(),
            RulesEntry {
                rules: BTreeMap::from([(
                    "00000000-0000-0000-0000-00000000000a".to_owned(),
                    verbatim(),
                )]),
                unknown: BTreeMap::new(),
            },
        )]));
        let sibling_clocked = sections_with_rules(BTreeMap::from([(
            "500".to_owned(),
            RulesEntry {
                rules: BTreeMap::from([(
                    "00000000-0000-0000-0000-00000000000a".to_owned(),
                    rule_entry(600),
                )]),
                unknown: BTreeMap::new(),
            },
        )]));
        let fragment = migrate_fragment(
            &verbatim_base,
            &present(&["500"]),
            &one_sibling(None, &sibling_clocked),
        );
        assert!(is_empty(&fragment), "verbatim base suppresses");
        // A verbatim sibling row never triggers, even against a missing
        // base key.
        let fragment = fragment_for(verbatim());
        assert!(is_empty(&fragment), "verbatim sibling never triggers");
        // A missing rule key triggers (clocked sibling, absent base key).
        let base_other_key = sections_with_rules(BTreeMap::from([(
            "500".to_owned(),
            RulesEntry {
                rules: BTreeMap::from([(
                    "00000000-0000-0000-0000-00000000000f".to_owned(),
                    rule_entry(500),
                )]),
                unknown: BTreeMap::new(),
            },
        )]));
        let sibling_new_key = sections_with_rules(BTreeMap::from([(
            "500".to_owned(),
            RulesEntry {
                rules: BTreeMap::from([(
                    "00000000-0000-0000-0000-00000000000a".to_owned(),
                    rule_entry(400),
                )]),
                unknown: BTreeMap::new(),
            },
        )]));
        let fragment = migrate_fragment(
            &base_other_key,
            &present(&["500"]),
            &one_sibling(None, &sibling_new_key),
        );
        assert!(
            fragment.reminder_rules.is_some(),
            "a missing rule key triggers"
        );
    }

    // T5: dismissals are a set union — missing rows migrate, present
    // rows never retrigger.
    #[test]
    fn dismissals_are_set_union_missing_rows_migrate_present_rows_never_retrigger() {
        let key_a = "00000000-0000-0000-0000-00000000000a|2026-01-01T08:00:00Z".to_owned();
        let key_b = "00000000-0000-0000-0000-00000000000a|2026-01-02T08:00:00Z".to_owned();
        let base = sections_with_dismissals(BTreeMap::from([(
            "500".to_owned(),
            DismissalsEntry {
                dismissals: BTreeMap::from([(
                    key_a.clone(),
                    DismissalEntry::Dismissed {
                        dismissed_at: ts(800),
                    },
                )]),
                unknown: BTreeMap::new(),
            },
        )]));
        let sibling = sections_with_dismissals(BTreeMap::from([(
            "500".to_owned(),
            DismissalsEntry {
                dismissals: BTreeMap::from([
                    (
                        key_a.clone(),
                        DismissalEntry::Dismissed {
                            dismissed_at: ts(800),
                        },
                    ),
                    (
                        key_b.clone(),
                        DismissalEntry::Dismissed {
                            dismissed_at: ts(900),
                        },
                    ),
                ]),
                unknown: BTreeMap::new(),
            },
        )]));
        let fragment = migrate_fragment(&base, &present(&["500"]), &one_sibling(None, &sibling));
        let entry = &fragment.dismissals.as_ref().unwrap()["500"];
        assert!(
            entry.dismissals.contains_key(&key_b),
            "the missing row migrates"
        );

        // Present rows only: nothing retriggers.
        let sibling_same = sections_with_dismissals(BTreeMap::from([(
            "500".to_owned(),
            DismissalsEntry {
                dismissals: BTreeMap::from([(
                    key_a.clone(),
                    DismissalEntry::Dismissed {
                        dismissed_at: ts(800),
                    },
                )]),
                unknown: BTreeMap::new(),
            },
        )]));
        let fragment =
            migrate_fragment(&base, &present(&["500"]), &one_sibling(None, &sibling_same));
        assert!(is_empty(&fragment), "present rows never retrigger");
    }

    // T7: the snapshot scoping — a sibling entry for a card the
    // destination never adopted contributes nothing.
    #[test]
    fn entries_for_cards_absent_from_the_destination_snapshot_are_ignored() {
        let sibling = document_with(sections_with_tasks(BTreeMap::from([(
            "999".to_owned(),
            task_entry(&[(
                "effort",
                write(
                    ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(900))),
                    500,
                ),
            )]),
        )])));
        let fragment = migrate_fragment(
            &AppDataSections::default(),
            &present(&["500"]),
            &one_sibling(Some(&sibling), &AppDataSections::default()),
        );
        assert!(
            is_empty(&fragment),
            "card 999 is in no destination snapshot"
        );
    }

    // T8: the reservoir shadows only where the live cache lacks the card.
    #[test]
    fn the_reservoir_shadows_only_where_the_live_cache_lacks_the_card() {
        let cached_entry = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(1))),
                500,
            ),
        )]);
        let reservoir_entry = task_entry(&[(
            "effort",
            write(
                ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(2))),
                500,
            ),
        )]);
        let cache = document_with(sections_with_tasks(BTreeMap::from([(
            "500".to_owned(),
            cached_entry,
        )])));
        let reservoir = sections_with_tasks(BTreeMap::from([
            ("500".to_owned(), reservoir_entry.clone()),
            (
                "501".to_owned(),
                task_entry(&[(
                    "effort",
                    write(
                        ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(3))),
                        500,
                    ),
                )]),
            ),
        ]));
        let fragment = migrate_fragment(
            &AppDataSections::default(),
            &present(&["500", "501"]),
            &one_sibling(Some(&cache), &reservoir),
        );
        let tasks = fragment.tasks.as_ref().unwrap();
        assert_eq!(
            tasks["500"].fields["effort"].value,
            ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(1))),
            "the live cache wins the card"
        );
        assert_eq!(
            tasks["501"].fields["effort"].value,
            ModeledValue::Effort(Some(taskboard_domain::entities::Effort::new(3))),
            "the reservoir serves the card the cache lacks"
        );
    }
}

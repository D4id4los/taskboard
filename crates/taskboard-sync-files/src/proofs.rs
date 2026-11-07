// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `proofs` wire section's codec (DR-9 D-BF3): the reminder
//! delivery proofs as set-union rows keyed `(rule, occurrence)`. The
//! `journal` codec is the template; the differences are the key (the
//! `dismissal_wire_key` injectivity argument — `"<ref>|<rfc3339
//! occurrence>"`, neither component can carry `|`) and the body
//! grammar (empty: reminder and occurrence are the key, `delivered_at`
//! is the row clock; the whole proof is the key plus the stamp).
//!
//! **Emission scope** (the dismissals split law): only `Rule`- and
//! `Break`-ref proofs roam — `Default`/`Adhoc`/`Tracker` occurrences
//! are per-device facts and never encode.
//!
//! The `author` slot is ignored on decode (it ranked in the fold). A
//! `Tombstone` is not a modeled verb (a proof is a settled fact; there
//! is no un-deliver) — rejected, relayed, counted, never imported.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use taskboard_domain::{ReminderDelivery, ReminderRef, ReminderState};

use crate::document::UserRecord;
use crate::journal::RowRejection;

/// The wire map key of one proof: `"<ref text form>|<rfc3339
/// occurrence>"`. Neither component can contain `|` (a ref's text form
/// is `<kind>:<uuid>`, rfc3339 is digits/punct), so the encoding is
/// injective.
#[must_use]
pub fn proof_wire_key(proof: &ReminderDelivery) -> String {
    format!(
        "{}|{}",
        proof.reminder.text_form(),
        proof.occurrence.to_rfc3339()
    )
}

/// Encodes one proof: `Entry` with an empty body (the key carries
/// reminder + occurrence; the clock is `delivered_at`), the author
/// slot empty (v1 never emits it).
///
/// # Panics
///
/// Never: the clock always serializes (a total-function pin).
#[must_use]
pub fn encode_proof(proof: &ReminderDelivery) -> (String, UserRecord) {
    (
        proof_wire_key(proof),
        UserRecord::Entry {
            body: BTreeMap::new(),
            clock: proof.delivered_at,
            author: None,
        },
    )
}

/// Encodes the delivered set's roaming slice: every `Rule`/`Break`-ref
/// proof, others filtered (the per-device facts stay local). An empty
/// slice omits the section (the `journal_section` absence rule).
#[must_use]
pub fn proofs_section(state: &ReminderState) -> Option<BTreeMap<String, UserRecord>> {
    let rows: BTreeMap<String, UserRecord> = state
        .deliveries
        .iter()
        .filter(|proof| matches!(proof.reminder, ReminderRef::Rule(_) | ReminderRef::Break(_)))
        .map(encode_proof)
        .collect();
    (!rows.is_empty()).then_some(rows)
}

/// Parses [`proof_wire_key`] back; `None` for any key this version
/// does not model (a `default:`/`alarm:`/`tracker:`-prefixed row a
/// hand-edited or newer-peer document might carry — the per-device
/// refs never roam, so decoding them would import a fact the owning
/// device never published; fail-closed).
#[must_use]
pub fn parse_proof_wire_key(key: &str) -> Option<(ReminderRef, DateTime<Utc>)> {
    let (ref_part, occurrence_part) = key.split_once('|')?;
    let reference = ReminderRef::parse_text_form(ref_part)?;
    if !matches!(reference, ReminderRef::Rule(_) | ReminderRef::Break(_)) {
        return None;
    }
    let occurrence = DateTime::parse_from_rfc3339(occurrence_part)
        .ok()?
        .with_timezone(&Utc);
    Some((reference, occurrence))
}

/// Decodes one keyed wire row into the import input: a live `Entry`
/// row with an empty body (anything else in the body is a newer
/// grammar this version does not understand — fail-closed, the relay
/// keeps it). The `author` slot is ignored (it ranked in the fold
/// already); `delivered_at` is the row clock.
///
/// # Errors
///
/// [`RowRejection::Verbatim`] for an unmodelable shape,
/// [`RowRejection::BadKey`] for a key outside the modeled ref families
/// or unparseable, and [`RowRejection::BadBody`] for a tombstone or a
/// non-empty body.
#[allow(clippy::double_must_use)]
pub fn decode_proof(key: &str, record: &UserRecord) -> Result<ReminderDelivery, RowRejection> {
    let (reference, occurrence) = parse_proof_wire_key(key).ok_or(RowRejection::BadKey)?;
    let UserRecord::Entry { body, clock, .. } = record else {
        return Err(match record {
            UserRecord::Verbatim(_) => RowRejection::Verbatim,
            UserRecord::Tombstone { .. } => RowRejection::BadBody,
            UserRecord::Entry { .. } => unreachable!("the else arm excludes entries"),
        });
    };
    if !body.is_empty() {
        return Err(RowRejection::BadBody);
    }
    Ok(ReminderDelivery {
        reminder: reference,
        occurrence,
        delivered_at: *clock,
    })
}

/// Decodes a whole keyed section into `(import inputs, rejected)`.
/// Order follows the map's key order (deterministic); rejections are
/// counted, never fatal.
#[allow(clippy::double_must_use)]
#[must_use]
pub fn decode_proofs(rows: &BTreeMap<String, UserRecord>) -> (Vec<ReminderDelivery>, usize) {
    let mut imported = Vec::new();
    let mut rejected = 0;
    for (key, record) in rows {
        match decode_proof(key, record) {
            Ok(proof) => imported.push(proof),
            Err(_) => rejected += 1,
        }
    }
    (imported, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use taskboard_domain::TaskId;
    use taskboard_domain::ids::{AdhocAlarmId, BreakRuleId, HabitId, ReminderRuleId};

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn rule_proof(occurrence: i64, delivered_at: i64) -> ReminderDelivery {
        ReminderDelivery {
            reminder: ReminderRef::Rule(ReminderRuleId::from(uuid::Uuid::from_u128(0x2f_1a))),
            occurrence: ts(occurrence),
            delivered_at: ts(delivered_at),
        }
    }

    #[test]
    fn a_proof_encodes_and_decodes_back_identically() {
        let original = rule_proof(1_000, 2_000);
        let (key, record) = encode_proof(&original);
        assert_eq!(decode_proof(&key, &record).expect("decodes"), original);
    }

    /// The key is injective and stable: ref text form + `|` + rfc3339.
    #[test]
    fn the_wire_key_is_the_ref_and_the_occurrence() {
        let proof = rule_proof(1_000, 2_000);
        let ReminderRef::Rule(rule) = proof.reminder else {
            panic!("fixture shape");
        };
        assert_eq!(
            proof_wire_key(&proof),
            format!("rule:{}|{}", rule.as_uuid(), ts(1_000).to_rfc3339())
        );
        // Distinct occurrences of one rule are distinct keys.
        let (key_a, _) = encode_proof(&rule_proof(1_000, 2_000));
        let (key_b, _) = encode_proof(&rule_proof(3_000, 4_000));
        assert_ne!(key_a, key_b);
    }

    /// A break-ref proof roams exactly like a rule-ref one.
    #[test]
    fn a_break_ref_proof_roundtrips() {
        let proof = ReminderDelivery {
            reminder: ReminderRef::Break(BreakRuleId::from(uuid::Uuid::from_u128(0x2f_1b))),
            occurrence: ts(5_000),
            delivered_at: ts(6_000),
        };
        let (key, record) = encode_proof(&proof);
        assert!(key.starts_with("break:"));
        assert_eq!(decode_proof(&key, &record).expect("decodes"), proof);
    }

    /// THE PIN: non-roaming refs never encode — the per-device facts
    /// (default reminders, adhoc alarms, habit nudges) stay local.
    #[test]
    fn non_roaming_refs_never_encode() {
        let mut state = ReminderState::default();
        for reference in [
            ReminderRef::Default(TaskId::from(uuid::Uuid::from_u128(1))),
            ReminderRef::Adhoc(AdhocAlarmId::from(uuid::Uuid::from_u128(2))),
            ReminderRef::Tracker(HabitId::from(uuid::Uuid::from_u128(3))),
        ] {
            state.deliveries.insert(ReminderDelivery {
                reminder: reference,
                occurrence: ts(1_000),
                delivered_at: ts(2_000),
            });
        }
        state.deliveries.insert(rule_proof(3_000, 4_000));
        let section = proofs_section(&state).expect("the rule proof encodes");
        assert_eq!(section.len(), 1, "only the Rule-ref proof roams");
    }

    /// An empty delivered set omits the section.
    #[test]
    fn an_empty_set_omits_the_section() {
        assert_eq!(proofs_section(&ReminderState::default()), None);
    }

    /// THE PIN (fail-closed): a non-roaming ref's key (a hand-edited
    /// or newer-peer document row) never imports — it would mint a
    /// fact the owning device never published.
    #[test]
    fn a_non_roaming_key_rejects_as_bad_key() {
        let record = UserRecord::Entry {
            body: BTreeMap::new(),
            clock: ts(2_000),
            author: None,
        };
        assert_eq!(
            decode_proof(
                "alarm:00000000-0000-0000-0000-000000000001|2026-01-01T00:00:00Z",
                &record
            ),
            Err(RowRejection::BadKey)
        );
        assert_eq!(decode_proof("garbage", &record), Err(RowRejection::BadKey));
    }

    /// A tombstone or a non-empty body rejects as `BadBody` (a newer
    /// grammar this version does not understand: relayed, counted,
    /// never imported).
    #[test]
    fn a_tombstone_or_carrying_body_rejects_as_bad_body() {
        let key = proof_wire_key(&rule_proof(1_000, 2_000));
        let tombstone = UserRecord::Tombstone {
            clock: ts(3_000),
            author: None,
        };
        assert_eq!(decode_proof(&key, &tombstone), Err(RowRejection::BadBody));
        let carrying = UserRecord::Entry {
            body: BTreeMap::from([("future_fact".to_owned(), serde_json::json!(true))]),
            clock: ts(2_000),
            author: None,
        };
        assert_eq!(decode_proof(&key, &carrying), Err(RowRejection::BadBody));
    }

    /// A verbatim row rejects as `Verbatim`.
    #[test]
    fn a_verbatim_row_rejects_as_verbatim() {
        let key = proof_wire_key(&rule_proof(1_000, 2_000));
        let record = UserRecord::Verbatim(serde_json::json!({"opaque": true}));
        assert_eq!(decode_proof(&key, &record), Err(RowRejection::Verbatim));
    }

    /// The whole section decodes; per-row verdicts never drag
    /// neighbours.
    #[test]
    fn the_section_decodes_whole_with_per_row_verdicts() {
        let mut section = BTreeMap::new();
        let (good_key, good) = encode_proof(&rule_proof(1_000, 2_000));
        section.insert(good_key, good);
        section.insert(
            "garbage".to_owned(),
            UserRecord::Verbatim(serde_json::json!(1)),
        );
        let (imported, rejected) = decode_proofs(&section);
        assert_eq!(rejected, 1);
        assert_eq!(imported, vec![rule_proof(1_000, 2_000)]);
    }
}

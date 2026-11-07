// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `settings` wire section's codec (DR-9 D-BF3): a device's
//! roaming settings home ↔ keyed [`UserRecord`] rows, so the
//! transport-agnostic core can encode and decode the section. The
//! `journal` codec is the template; the differences are the key (the
//! setting's stable name, not a UUID) and the body grammar
//! (`{"value": …}`, minted through
//! [`RoamingSettingValue::decode`] — an unknown key or an unmintable
//! value rejects the row, typed, never fatal).
//!
//! The `author` slot is ignored on decode (it already ranked in the
//! fold; the engine adopts winners verbatim). The row's `clock` is the
//! per-key write clock — the only competition metadata, exactly the
//! section recipe.
//!
//! There is deliberately **no encode of an unknown key**: the home's
//! type system cannot hold one, so a future build's roamer relays
//! through the fold's verbatim rows and never imports here. And
//! settings have no tombstone verb — a device's return to a default is
//! a *write* of that default, not a delete (a tombstone row therefore
//! rejects, relayed and counted, never imported).

use std::collections::BTreeMap;

use taskboard_domain::{ImportedSetting, RoamingSettingRow, RoamingSettingValue, RoamingSettings};

use crate::document::UserRecord;
use crate::journal::RowRejection;

/// The wire map key of a setting: its stable name
/// ([`RoamingSettingValue::key_name`]).
#[must_use]
pub fn setting_key(value: &RoamingSettingValue) -> String {
    value.key_name().to_owned()
}

/// Encodes one roaming row: `Entry` with `{"value": …}` as the body,
/// the row's clock as the clock, the author slot empty (v1 never emits
/// it).
///
/// # Panics
///
/// Never: the value always serializes (a total-function pin).
#[must_use]
pub fn encode_setting(row: &RoamingSettingRow) -> (String, UserRecord) {
    (
        setting_key(&row.value),
        UserRecord::Entry {
            body: BTreeMap::from([(
                "value".to_owned(),
                serde_json::to_value(row.value).expect("a setting value always serializes"),
            )]),
            clock: row.clock,
            author: None,
        },
    )
}

/// Encodes the whole roaming home; an empty home omits the section
/// (the `journal_section` absence rule).
#[must_use]
pub fn settings_section(home: &RoamingSettings) -> Option<BTreeMap<String, UserRecord>> {
    (!home.0.is_empty()).then(|| home.0.values().map(encode_setting).collect())
}

/// Decodes one keyed wire row into an import input. The `author` slot
/// is ignored (it ranked in the fold already).
///
/// # Errors
///
/// [`RowRejection::Verbatim`] for an unmodelable shape,
/// [`RowRejection::BadKey`] for a key outside the pinned roam-set, and
/// [`RowRejection::BadBody`] when the body is not a `{"value": …}`
/// shape or the value fails its own mint.
#[allow(clippy::double_must_use)]
pub fn decode_setting(key: &str, record: &UserRecord) -> Result<ImportedSetting, RowRejection> {
    let UserRecord::Entry { body, clock, .. } = record else {
        return Err(match record {
            UserRecord::Verbatim(_) => RowRejection::Verbatim,
            UserRecord::Tombstone { .. } => RowRejection::BadBody,
            UserRecord::Entry { .. } => unreachable!("the else arm excludes entries"),
        });
    };
    // Key first: a name outside the pinned roam-set is a BadKey (a
    // future build's roamer), a known name with an unmintable body a
    // BadBody — the two failures name different repairs.
    if !taskboard_domain::ROAMING_SETTING_NAMES.contains(&key) {
        return Err(RowRejection::BadKey);
    }
    let Some(value) = body.get("value") else {
        return Err(RowRejection::BadBody);
    };
    let value = RoamingSettingValue::decode(key, value).ok_or(RowRejection::BadBody)?;
    Ok(ImportedSetting {
        value,
        clock: *clock,
    })
}

/// Decodes a whole keyed section into `(import inputs, rejected)`.
/// Order follows the map's key order (deterministic); rejections are
/// counted, never fatal.
#[allow(clippy::double_must_use)]
#[must_use]
pub fn decode_settings(rows: &BTreeMap<String, UserRecord>) -> (Vec<ImportedSetting>, usize) {
    let mut imported = Vec::new();
    let mut rejected = 0;
    for (key, record) in rows {
        match decode_setting(key, record) {
            Ok(row) => imported.push(row),
            Err(_) => rejected += 1,
        }
    }
    (imported, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ts(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn row(value: RoamingSettingValue, clock: i64) -> RoamingSettingRow {
        RoamingSettingRow {
            value,
            clock: ts(clock),
        }
    }

    #[test]
    fn a_row_encodes_and_decodes_back_identically() {
        let original = row(
            RoamingSettingValue::DayStartTime(chrono::NaiveTime::from_hms_opt(5, 30, 0).unwrap()),
            1_000,
        );
        let (key, record) = encode_setting(&original);
        assert_eq!(key, "day_start_time");
        let decoded = decode_setting(&key, &record).expect("decodes");
        assert_eq!(decoded.value, original.value);
        assert_eq!(decoded.clock, original.clock);
    }

    /// The wire row's shape: exactly the `value` body key plus the
    /// modeled clock metadata; no author key ever emitted (the v1 pin).
    #[test]
    fn the_encoded_row_is_exactly_value_plus_clock() {
        let (_, record) = encode_setting(&row(RoamingSettingValue::FocusChain(true), 42));
        let json = serde_json::to_value(record).unwrap();
        let map = json.as_object().unwrap();
        assert_eq!(map.len(), 2, "exactly value + clock ride the row");
        assert_eq!(map["value"], serde_json::json!({"FocusChain": true}));
        assert!(map.contains_key("clock"));
        assert!(!map.contains_key("author"));
    }

    /// A future build's unknown key rejects as `BadKey` (relayed,
    /// counted, never an error).
    #[test]
    fn an_unknown_key_rejects_as_bad_key() {
        let (_, record) = encode_setting(&row(RoamingSettingValue::FocusChain(true), 42));
        assert_eq!(
            decode_setting("focus_ambient", &record),
            Err(RowRejection::BadKey)
        );
    }

    /// A body missing the `value` key rejects as `BadBody`; a value that
    /// fails its own mint too (fail-closed, never a silent default).
    #[test]
    fn an_unmintable_body_rejects_as_bad_body() {
        let (key, mut record) = encode_setting(&row(RoamingSettingValue::FocusChain(true), 42));
        let UserRecord::Entry { body, .. } = &mut record else {
            panic!("an encoded setting is an Entry");
        };
        body.clear();
        assert_eq!(decode_setting(&key, &record), Err(RowRejection::BadBody));
        let record = UserRecord::Entry {
            body: BTreeMap::from([("value".to_owned(), serde_json::json!("not-a-time"))]),
            clock: ts(42),
            author: None,
        };
        assert_eq!(
            decode_setting("day_start_time", &record),
            Err(RowRejection::BadBody)
        );
    }

    /// A tombstone is not a settings verb: rejected as `BadBody` (the
    /// document relays it; the import never sees it).
    #[test]
    fn a_tombstone_rejects_as_bad_body() {
        let record = UserRecord::Tombstone {
            clock: ts(42),
            author: None,
        };
        assert_eq!(
            decode_setting("focus_chain", &record),
            Err(RowRejection::BadBody)
        );
    }

    /// A verbatim row rejects as `Verbatim` (the relay keeps it; the
    /// engine never sees it).
    #[test]
    fn a_verbatim_row_rejects_as_verbatim() {
        let record = UserRecord::Verbatim(serde_json::json!({"opaque": true}));
        assert_eq!(
            decode_setting("focus_chain", &record),
            Err(RowRejection::Verbatim)
        );
    }

    /// The whole home encodes; an empty home omits the section.
    #[test]
    fn the_section_encodes_and_decodes_whole() {
        let mut home = RoamingSettings::default();
        assert_eq!(settings_section(&home), None);
        home.0.insert(
            "focus_chain".to_owned(),
            row(RoamingSettingValue::FocusChain(true), 10),
        );
        home.0.insert(
            "due_soon_days".to_owned(),
            row(
                RoamingSettingValue::DueSoonDays(taskboard_domain::DueSoonDays::new(3).unwrap()),
                20,
            ),
        );
        let section = settings_section(&home).expect("non-empty");
        assert_eq!(section.len(), 2);
        let (imported, rejected) = decode_settings(&section);
        assert_eq!(rejected, 0);
        assert_eq!(imported.len(), 2);
    }

    /// A garbled row inside a well-formed section is counted, never
    /// fatal, and never drags its neighbours (the verdicts are
    /// per-row).
    #[test]
    fn a_garbled_row_is_counted_and_never_fatal() {
        let mut section = BTreeMap::new();
        let (good_key, good) = encode_setting(&row(RoamingSettingValue::FocusNotify(true), 10));
        section.insert(good_key, good);
        section.insert(
            "focus_chain".to_owned(),
            UserRecord::Verbatim(serde_json::json!(1)),
        );
        section.insert(
            "reminders_enabled".to_owned(),
            UserRecord::Entry {
                body: BTreeMap::from([("value".to_owned(), serde_json::json!("not-a-bool"))]),
                clock: ts(10),
                author: None,
            },
        );
        let (imported, rejected) = decode_settings(&section);
        assert_eq!(rejected, 2);
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].value, RoamingSettingValue::FocusNotify(true));
    }
}

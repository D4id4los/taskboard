// SPDX-License-Identifier: MIT OR Apache-2.0

//! The time-type seam (#987).
//!
//! A single retarget point for the workspace's time types: the aliases
//! below name the exact chrono types (an alias *is* the same type —
//! type-identical by construction, stronger than behavior-identical),
//! and the thin function wrappers cover the parse/duration operations
//! a migration would have to translate.
//!
//! # Retarget policy
//!
//! A future migration (jiff/time, when a #891 revisit trigger fires)
//! rewrites *this module* plus the call sites that route through it —
//! one seam instead of a workspace-wide sweep of ~2700 `chrono::`
//! occurrences. Everything else stays direct chrono, by decision
//! (2026-11-24, the #891 ruling: stay on chrono; the seam is the
//! cheapened exit):
//!
//! - **Calendar arithmetic** (`recur`, `days` logic, `habit` —
//!   `Month`/`Weekday`/`Days`): migration-sensitive semantics; only
//!   retargeted if a migration happens.
//! - **UI formatting**, `Local`/`FixedOffset` tz seams, serialization
//!   boundaries (serde derives, sqlite RFC 3339 codecs, wire formats):
//!   the migration's tail, decided wholesale at migration time.
//! - New chrono call sites do **not** retroactively migrate; the seam
//!   grows only when a site migrates to it.
//!
//! In-scope seam callers today: [`crate::clock`] (the `Clock`
//! signature), `sync-nextcloud`'s `parse_retry_after` (HTTP-date
//! parsing), `sync-files-nextcloud`'s etag skew arithmetic, and the
//! signature-level aliases in [`crate::days`].
//!
//! The wrappers are transparent: [`delta_seconds`] equals
//! `chrono::TimeDelta::seconds` and [`parse_http_date`] round-trips an
//! IMF-fixdate header — asserted in the tests below, because a seam
//! that transforms is a bug.

/// An instant in UTC — `chrono::DateTime<chrono::Utc>` under its
/// migration-facing name.
pub type Timestamp = chrono::DateTime<chrono::Utc>;

/// A calendar date without a zone — `chrono::NaiveDate` under its
/// migration-facing name.
pub type LocalDate = chrono::NaiveDate;

/// A wall-clock time of day without a zone — `chrono::NaiveTime` under
/// its migration-facing name.
pub type LocalTime = chrono::NaiveTime;

/// A signed span of civil time — `chrono::TimeDelta` under its
/// migration-facing name.
pub type Delta = chrono::TimeDelta;

/// A delta of exactly `secs` whole seconds. Transparent: identical to
/// `chrono::TimeDelta::seconds(secs)` (panics on out-of-range, same as
/// chrono — never silently saturates).
#[must_use]
pub fn delta_seconds(secs: i64) -> Delta {
    chrono::TimeDelta::seconds(secs)
}

/// Parses an HTTP-date header value (`Retry-After`'s IMF-fixdate form,
/// RFC 2822-compatible) into a UTC [`Timestamp`]. `None` — absent,
/// never an error — for anything unparsable. Transparent: identical
/// parse (chrono's RFC 2822 path) and identical UTC conversion.
#[must_use]
pub fn parse_http_date(s: &str) -> Option<Timestamp> {
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Transparency unit test: the wrappers must be indistinguishable
    // from the direct chrono calls they stand in for — a seam that
    // transforms is a bug.
    #[test]
    fn delta_seconds_is_transparent() {
        assert_eq!(delta_seconds(2), chrono::Duration::seconds(2));
        assert_eq!(delta_seconds(0), chrono::Duration::zero());
        assert_eq!(delta_seconds(-7), chrono::TimeDelta::seconds(-7));
    }

    #[test]
    fn parse_http_date_round_trips_imf_fixdate() {
        use chrono::TimeZone;
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 11, 24, 9, 30, 0)
            .unwrap();
        let header = "Tue, 24 Nov 2026 10:30:00 GMT";
        let parsed = parse_http_date(header).expect("IMF-fixdate parses");
        assert_eq!(parsed, now + delta_seconds(3600));
        // Round-trip: chrono's own IMF-fixdate formatter output re-parses
        // to the same instant.
        let rendered = parsed.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert_eq!(parse_http_date(&rendered), Some(parsed));
        // Transparent against the direct chrono path.
        let direct = chrono::DateTime::parse_from_rfc2822(header)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc));
        assert_eq!(Some(parsed), direct);
    }

    #[test]
    fn parse_http_date_rejects_garbage_as_absent() {
        for absent in ["", "120", "not a date", "2026-11-24T10:30:00Z"] {
            assert_eq!(parse_http_date(absent), None, "absent for {absent:?}");
        }
    }
}

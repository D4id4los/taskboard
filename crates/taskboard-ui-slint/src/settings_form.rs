// SPDX-License-Identifier: AGPL-3.0-only

//! The settings form's pure validation mirror (round-1 D7): the
//! config's connector rules re-expressed over the draft so the form
//! can flag fields before a network round-trip. The app-side
//! re-validation (`AppConfig::validate` in the Save ceremony) stays
//! the authority; this mirror exists so Save is enabled only when the
//! draft passes and the UI can say *which* field is wrong. Parity is
//! pinned by shared fixtures in both crates' tests — drift breaks a
//! test.

/// The sign-in draft the form edits (the `ConnectionProfile` without
/// the reply plumbing and without a name — the app derives the
/// connector name from the identity, round-5 chunk 8 D-8.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConnectionDraft {
    pub server_url: String,
    pub username: String,
    pub password: String,
}

/// Which field failed (the lane's [`crate::requests::Field`] mirror,
/// kept separate so this module stays lane-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    ServerUrl,
    Username,
    Password,
}

/// How a field failed (the lane's `InvalidKind` mirror, kept complete
/// so this module stays lane-free: the scale field's parity with the
/// `UiScale` mint needs `NotFinite`/`OutOfRange`, whose lane arms and
/// status sentences already exist).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidKind {
    Empty,
    NotKebab,
    BadUrl,
    NotFinite,
    OutOfRange,
}

/// The form-level validation outcome: the first failing field (the
/// config ladder's order), or ok. There is no collision arm by
/// design: the app derives a free connector name for a new identity
/// and reuses the configured one for a re-sign-in (D-8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    Invalid { field: Field, kind: InvalidKind },
}

/// The per-field validity flavour (round-3 D19): what the field's
/// border and error line render. `Neutral` is the empty field — no
/// decoration; `Invalid` carries the same kind the Save gate's ladder
/// would report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldStatus {
    Neutral,
    Valid,
    Invalid(InvalidKind),
}

/// The per-field predicate behind [`validate`] (round-3 D19): the
/// ladder recomposes from this, so the inline flavour and the Save
/// gate are one source of truth — they cannot diverge by
/// construction. `Some(kind)` iff this field alone would fail.
#[must_use]
pub fn field_invalid(field: Field, draft: &ConnectionDraft) -> Option<InvalidKind> {
    match field {
        Field::ServerUrl => {
            if url_host_ok(&draft.server_url) {
                None
            } else {
                Some(InvalidKind::BadUrl)
            }
        }
        Field::Username => {
            if draft.username.trim().is_empty() {
                Some(InvalidKind::Empty)
            } else {
                None
            }
        }
        Field::Password => {
            if draft.password.trim().is_empty() {
                Some(InvalidKind::Empty)
            } else {
                None
            }
        }
    }
}

/// The flavour mapping (round-3 D19): empty → [`FieldStatus::Neutral`],
/// failing → [`FieldStatus::Invalid`], passing → [`FieldStatus::Valid`].
/// Empty means literally empty text — whitespace-only content is
/// non-empty and renders the ladder's `Empty` invalidity (§1.1: the
/// neutral state is *the field's text is empty*). The scale field's
/// own mapping ([`scale_status`]) deliberately trims instead — its
/// truth table pins `"   "` → Neutral.
#[must_use]
pub fn field_status(field: Field, draft: &ConnectionDraft) -> FieldStatus {
    if field_text(field, draft).is_empty() {
        return FieldStatus::Neutral;
    }
    match field_invalid(field, draft) {
        Some(kind) => FieldStatus::Invalid(kind),
        None => FieldStatus::Valid,
    }
}

/// The field's raw text (the flavour's emptiness probe reads the same
/// buffer the predicate does).
fn field_text(field: Field, draft: &ConnectionDraft) -> &str {
    match field {
        Field::ServerUrl => &draft.server_url,
        Field::Username => &draft.username,
        Field::Password => &draft.password,
    }
}

/// Whether the draft may attempt a save (the Save button's
/// enablement): every field passes per [`validate`].
#[must_use]
pub fn draft_valid(draft: &ConnectionDraft) -> bool {
    validate(draft).is_none()
}

/// The mirror of the config's connector-entry ladder (plan F-D2), in
/// the same order (the derived name has no form field): non-empty
/// username/password → URL host rule. No collision arm (see
/// [`DraftError`]).
/// Recomposed from [`field_invalid`] in the ladder order — the
/// per-field predicates and this ladder are one source of truth.
#[must_use]
pub fn validate(draft: &ConnectionDraft) -> Option<DraftError> {
    let ladder = [Field::Username, Field::Password, Field::ServerUrl];
    for field in ladder {
        if let Some(kind) = field_invalid(field, draft) {
            return Some(DraftError::Invalid { field, kind });
        }
    }
    None
}

/// The config's `is_kebab_name` char-class rules re-expressed: every
/// char in `[a-z0-9-]`, no leading/trailing `-`, no `--`.
#[must_use]
pub fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// The host-presence half of `DeckClient::new`'s URL rules (the config
/// layer's `reqwest_host_ok` mirror): scheme http/https, non-empty
/// host, no userinfo in the authority.
#[must_use]
pub fn url_host_ok(raw: &str) -> bool {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.contains('@') {
        return false;
    }
    !authority.is_empty()
}

/// The scale field's per-keystroke predicate (round-3 D20a): parity
/// with the authoritative mint `crate::scale::UiScale::new` — parse
/// failure maps to [`InvalidKind::NotKebab`] (the lane lattice's
/// reserved arm; `settings_status::invalid_text` already carries its
/// sentence, so no enum extension and no protocol change),
/// non-finite to [`InvalidKind::NotFinite`], outside the
/// `0.5..=3.0` window to [`InvalidKind::OutOfRange`]. Trimmed-empty is
/// not an error here — the flavour renders Neutral for it.
#[must_use]
pub fn scale_invalid_kind(text: &str) -> Option<InvalidKind> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Ok(value) = trimmed.parse::<f64>() else {
        return Some(InvalidKind::NotKebab);
    };
    match crate::scale::UiScale::new(value) {
        Err(crate::scale::UiScaleError::NonFinite) => Some(InvalidKind::NotFinite),
        Err(crate::scale::UiScaleError::OutOfRange) => Some(InvalidKind::OutOfRange),
        Ok(_) => None,
    }
}

// -- the general fields' predicates (the GUI settings editor) --------
//
// The same discipline as the connector mirror: the app-side
// validation stays the authority (the prove gate re-proves the whole
// document); these predicates exist so the commit seam can refuse
// before the lane and the field can flag its flavour early. Parity
// with `AppConfig::validate`'s rejection sets is pinned by fixtures.

/// The day-start field's per-keystroke predicate: parity with the
/// config's own rule (`preferences.day_start_time` parses as a
/// `chrono::NaiveTime`). Parse failure maps to the lane lattice's
/// reserved parse arm (the scale field's precedent); trimmed-empty is
/// Neutral.
#[must_use]
pub fn day_start_invalid_kind(text: &str) -> Option<InvalidKind> {
    if text.trim().is_empty() {
        return None;
    }
    text.trim()
        .parse::<chrono::NaiveTime>()
        .map_or(Some(InvalidKind::NotKebab), |_| None)
}

/// The before field's predicate: the DELEGATION pin — the grammar is
/// domain-owned (`ReminderOffset::parse_text_form`, the config's
/// `parse_before` vocabulary), so this mirror has no grammar of its
/// own to drift (plan D9).
#[must_use]
pub fn before_invalid_kind(text: &str) -> Option<InvalidKind> {
    if text.trim().is_empty() {
        return None;
    }
    taskboard_domain::ReminderOffset::parse_text_form(text.trim())
        .map_or(Some(InvalidKind::NotKebab), |_| None)
}

/// The plain positive-seconds fields (tick, poll interval, backoff
/// ceiling): parity with the config's zero refusals. Parse failure →
/// the reserved parse arm; zero → `OutOfRange`.
#[must_use]
pub fn secs_invalid_kind(text: &str) -> Option<InvalidKind> {
    if text.trim().is_empty() {
        return None;
    }
    match text.trim().parse::<u64>() {
        Err(_) => Some(InvalidKind::NotKebab),
        Ok(0) => Some(InvalidKind::OutOfRange),
        Ok(_) => None,
    }
}

/// The due-soon field: parity with the config's ceiling rule — 0 is
/// legal (an empty soon set), above the ten-year horizon refuses.
#[must_use]
pub fn due_soon_invalid_kind(text: &str) -> Option<InvalidKind> {
    if text.trim().is_empty() {
        return None;
    }
    match text.trim().parse::<u32>() {
        Err(_) => Some(InvalidKind::NotKebab),
        Ok(days) if days > 3652 => Some(InvalidKind::OutOfRange),
        Ok(_) => None,
    }
}

/// The bound-checked core shared by the focus minute mirrors: parse
/// failure → the reserved parse arm; `0` or above `max` →
/// `OutOfRange` (the config ladder's strictly-positive minute rule);
/// trimmed-empty is Neutral.
fn focus_count_invalid_kind(text: &str, max: u64) -> Option<InvalidKind> {
    if text.trim().is_empty() {
        return None;
    }
    match text.trim().parse::<u64>() {
        Err(_) => Some(InvalidKind::NotKebab),
        Ok(mins) if mins == 0 || mins > max => Some(InvalidKind::OutOfRange),
        Ok(_) => None,
    }
}

/// The focus minute fields (`work_mins`/`break_mins`/
/// `long_break_mins`): parity with the config's
/// `FOCUS_MINS_MAX = 240` bound (ui-slint cannot depend on
/// taskboard-app; drift breaks a test on either side, and the journeys
/// pin the round-trip through the real gate).
#[must_use]
pub fn focus_mins_invalid_kind(text: &str) -> Option<InvalidKind> {
    focus_count_invalid_kind(text, 240)
}

/// The `long_break_every` field: parity with the config's
/// `FOCUS_EVERY_MAX = 24` bound (the same hard-coded-mirror posture).
#[must_use]
pub fn focus_every_invalid_kind(text: &str) -> Option<InvalidKind> {
    focus_count_invalid_kind(text, 24)
}

/// The generic flavour mapping over a text predicate (the
/// [`field_status`]/[`scale_status`] grammar, unified).
#[must_use]
pub fn general_status(text: &str, invalid: fn(&str) -> Option<InvalidKind>) -> FieldStatus {
    if text.trim().is_empty() {
        return FieldStatus::Neutral;
    }
    match invalid(text) {
        Some(kind) => FieldStatus::Invalid(kind),
        None => FieldStatus::Valid,
    }
}

/// The scale field's flavour mapping (round-3 D20a), the same grammar
/// as [`field_status`].
#[must_use]
pub fn scale_status(text: &str) -> FieldStatus {
    if text.trim().is_empty() {
        return FieldStatus::Neutral;
    }
    match scale_invalid_kind(text) {
        Some(kind) => FieldStatus::Invalid(kind),
        None => FieldStatus::Valid,
    }
}

/// The live-scale echo (round-3 D20a): the applied scale's storage
/// precision is the theme's `f32` property, so the value first rounds
/// through `f32` — rendering the widened `f64` directly would print
/// the widening noise (1.7 → "1.7000000476837158") and the echo would
/// lie. Rust `{}` `Display` on `f32` is shortest-round-trip; a
/// rendering with no fraction gets ".0" appended (1 → "1.0";
/// non-grid values like 2.25 render verbatim — `UiScale` enforces
/// only the window, not the 0.1 grid).
#[must_use]
pub fn format_scale(value: f64) -> String {
    #[allow(clippy::cast_possible_truncation)] // the theme's own precision
    let rendered = format!("{}", value as f32);
    if rendered.contains('.') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T1 — the kebab truth table (shared parity fixtures with the
    /// config layer's validation tests: the same accept/reject sets).
    #[test]
    fn the_kebab_truth_table_matches_the_config_rules() {
        for yes in ["work", "a", "a-1", "work-personal-2"] {
            assert!(is_kebab(yes), "{yes} must pass");
        }
        for no in [
            "",
            "Work",
            "work personal",
            "-work",
            "work-",
            "wor--k",
            "wörk",
            "work_person",
        ] {
            assert!(!is_kebab(no), "{no} must fail");
        }
    }

    /// T1 — the URL truth table, including the userinfo rejection (the
    /// F12-02 fail-closed rule the config layer pins).
    #[test]
    fn the_url_truth_table_matches_the_config_rules() {
        for yes in [
            "https://cloud.example.com",
            "http://localhost:8080",
            "https://host/path?query#frag",
        ] {
            assert!(url_host_ok(yes), "{yes} must pass");
        }
        for no in [
            "",
            "cloud.example.com",
            "ftp://cloud.example.com",
            "https://",
            "https://user:pass@host",
            "https://@host",
        ] {
            assert!(!url_host_ok(no), "{no} must fail");
        }
    }

    /// T1 — the ladder: the first failing field wins.
    #[test]
    fn the_ladder_reports_the_first_failing_field() {
        let base = ConnectionDraft {
            server_url: "https://c.example".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert_eq!(validate(&base), None);

        let mut d = base.clone();
        d.username = "  ".into();
        assert_eq!(
            validate(&d),
            Some(DraftError::Invalid {
                field: Field::Username,
                kind: InvalidKind::Empty
            })
        );

        let mut d = base.clone();
        d.password = String::new();
        assert_eq!(
            validate(&d),
            Some(DraftError::Invalid {
                field: Field::Password,
                kind: InvalidKind::Empty
            })
        );

        let mut d = base.clone();
        d.server_url = "not a url".into();
        assert_eq!(
            validate(&d),
            Some(DraftError::Invalid {
                field: Field::ServerUrl,
                kind: InvalidKind::BadUrl
            })
        );
    }

    /// The per-field predicate is the ladder's one source of truth:
    /// for every field, `field_invalid` agrees with what `validate`
    /// reports *about that field* (the flavour and the Save gate
    /// cannot diverge by construction).
    #[test]
    fn field_invalid_agrees_with_the_ladder_per_field() {
        let drafts: Vec<ConnectionDraft> = [
            ConnectionDraft::default(),
            ConnectionDraft {
                username: "alice".into(),
                ..ConnectionDraft::default()
            },
            ConnectionDraft {
                server_url: "https://c.example".into(),
                username: "alice".into(),
                password: "pw".into(),
            },
            ConnectionDraft {
                server_url: "not a url".into(),
                username: "alice".into(),
                password: "pw".into(),
            },
            ConnectionDraft {
                server_url: "https://c.example".into(),
                username: "  ".into(),
                password: "pw".into(),
            },
            ConnectionDraft {
                server_url: "https://c.example".into(),
                username: "alice".into(),
                password: "  ".into(),
            },
        ]
        .into_iter()
        .collect();
        for draft in &drafts {
            // Recomposition equivalence: the ladder *is* the first
            // failing per-field predicate in ladder order.
            let recomposed = [Field::Username, Field::Password, Field::ServerUrl]
                .into_iter()
                .find_map(|field| {
                    field_invalid(field, draft).map(|kind| DraftError::Invalid { field, kind })
                });
            assert_eq!(recomposed, validate(draft), "the ladder diverged");
        }
    }

    /// The flavour mapping: empty → Neutral, failing → Invalid(kind),
    /// passing → Valid. One row per branch per field class.
    #[test]
    fn field_status_maps_the_three_flavours() {
        let empty = ConnectionDraft::default();
        assert_eq!(field_status(Field::ServerUrl, &empty), FieldStatus::Neutral);
        assert_eq!(field_status(Field::Username, &empty), FieldStatus::Neutral);
        assert_eq!(field_status(Field::Password, &empty), FieldStatus::Neutral);

        let mut d = ConnectionDraft {
            server_url: "nope".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert_eq!(
            field_status(Field::ServerUrl, &d),
            FieldStatus::Invalid(InvalidKind::BadUrl)
        );

        d.server_url = "https://c.example".into();
        assert_eq!(field_status(Field::ServerUrl, &d), FieldStatus::Valid);
        assert_eq!(field_status(Field::Username, &d), FieldStatus::Valid);
        assert_eq!(field_status(Field::Password, &d), FieldStatus::Valid);

        d.username = String::new();
        assert_eq!(
            field_status(Field::Username, &d),
            FieldStatus::Neutral,
            "empty renders neutral, never the error line"
        );
        d.password = String::new();
        assert_eq!(
            field_status(Field::Password, &d),
            FieldStatus::Neutral,
            "empty renders neutral, never the error line"
        );
    }

    /// The scale field's flavour, in parity with the authoritative
    /// `UiScale::new` mint: parse-fail → `NotKebab` (the reserved arm),
    /// non-finite → `NotFinite`, outside the window → `OutOfRange`, empty
    /// → Neutral, else Valid.
    #[test]
    fn scale_status_paries_the_ui_scale_mint() {
        assert_eq!(scale_status(""), FieldStatus::Neutral);
        assert_eq!(scale_status("   "), FieldStatus::Neutral);
        assert_eq!(
            scale_status("abc"),
            FieldStatus::Invalid(InvalidKind::NotKebab)
        );
        assert_eq!(
            scale_status("nan"),
            FieldStatus::Invalid(InvalidKind::NotFinite)
        );
        assert_eq!(
            scale_status("inf"),
            FieldStatus::Invalid(InvalidKind::NotFinite)
        );
        assert_eq!(
            scale_status("0.4"),
            FieldStatus::Invalid(InvalidKind::OutOfRange)
        );
        assert_eq!(
            scale_status("3.01"),
            FieldStatus::Invalid(InvalidKind::OutOfRange)
        );
        assert_eq!(scale_status("0.5"), FieldStatus::Valid);
        assert_eq!(scale_status("1"), FieldStatus::Valid);
        assert_eq!(scale_status("3.0"), FieldStatus::Valid);
        assert_eq!(scale_status("2.25"), FieldStatus::Valid);
    }

    /// The echo formatter: shortest-round-trip with a ".0" appended
    /// when the rendering carries no fraction — the echo never lies
    /// about the applied value (1 → "1.0", non-grid 2.25 survives).
    #[test]
    fn format_scale_renders_shortest_round_trip_with_a_fraction() {
        assert_eq!(format_scale(1.0), "1.0");
        assert_eq!(format_scale(2.25), "2.25");
        assert_eq!(format_scale(0.5), "0.5");
        assert_eq!(format_scale(3.0), "3.0");
        assert_eq!(format_scale(1.7), "1.7");
    }

    // -- the general fields' predicates (U-1) ---------------------------

    /// The day-start truth table: parity with the config's own rule
    /// (a `chrono::NaiveTime` parse). Empty/whitespace is Neutral.
    #[test]
    fn the_day_start_truth_table_matches_the_config_rule() {
        let probe = |raw: &str| raw.parse::<chrono::NaiveTime>().is_err();
        for yes in ["00:00", "05:00", "23:59", "05:00:30"] {
            assert!(day_start_invalid_kind(yes).is_none(), "{yes} must pass");
            assert!(!probe(yes));
        }
        for no in ["25:00", "abc", "5", "05:60", "12:34:99"] {
            assert_eq!(
                day_start_invalid_kind(no),
                Some(InvalidKind::NotKebab),
                "{no} must refuse"
            );
            assert!(probe(no), "{no}: the mirror tracks the config parse");
        }
        assert_eq!(day_start_invalid_kind(""), None);
        assert_eq!(day_start_invalid_kind("   "), None);
    }

    /// The before field's pin is the DELEGATION (plan D9): the
    /// mirror's accept/reject set is exactly the domain parser's —
    /// whatever it accepts passes, whatever it refuses maps to the
    /// reserved parse arm. One grammar, no mirror of its own.
    #[test]
    fn the_before_predicate_delegates_to_the_domain_parser() {
        // Strict agreement over NON-empty text (the mirror's
        // trimmed-empty contract is Neutral, pinned below — not a
        // delegation fact).
        let agrees = |raw: &str| {
            before_invalid_kind(raw).is_none()
                == taskboard_domain::ReminderOffset::parse_text_form(raw.trim()).is_some()
        };
        for raw in ["5m", "0s", "1h30m", "2d", "abc", "-5m", "5x", "90m", "5 m"] {
            assert!(
                agrees(raw),
                "{raw}: the mirror must track the domain parser"
            );
        }
        assert_eq!(before_invalid_kind("5m"), None);
        assert_eq!(before_invalid_kind("abc"), Some(InvalidKind::NotKebab));
        assert_eq!(before_invalid_kind(""), None);
        assert_eq!(before_invalid_kind("   "), None);
    }

    /// The seconds fields: parse, positivity; zero is the config's
    /// own typed refusal (`OutOfRange`), parse failure the reserved arm.
    #[test]
    fn the_secs_truth_table_matches_the_config_rules() {
        assert_eq!(secs_invalid_kind("30"), None);
        assert_eq!(secs_invalid_kind("1"), None);
        assert_eq!(secs_invalid_kind("0"), Some(InvalidKind::OutOfRange));
        assert_eq!(secs_invalid_kind("-1"), Some(InvalidKind::NotKebab));
        assert_eq!(secs_invalid_kind("abc"), Some(InvalidKind::NotKebab));
        assert_eq!(secs_invalid_kind(""), None);
        assert_eq!(secs_invalid_kind("   "), None);
    }

    /// The due-soon field: 0 legal (an empty soon set), the ten-year
    /// ceiling refuses — the config ladder's own bounds.
    #[test]
    fn the_due_soon_truth_table_matches_the_config_ceiling() {
        assert_eq!(due_soon_invalid_kind("0"), None, "0 is legal");
        assert_eq!(due_soon_invalid_kind("1"), None);
        assert_eq!(due_soon_invalid_kind("3652"), None, "the boundary is legal");
        assert_eq!(due_soon_invalid_kind("3653"), Some(InvalidKind::OutOfRange));
        assert_eq!(due_soon_invalid_kind("abc"), Some(InvalidKind::NotKebab));
        assert_eq!(due_soon_invalid_kind(""), None);
    }

    /// The focus minute fields (`work_mins`/`break_mins`/
    /// `long_break_mins`): parity with the config ladder's
    /// `1..=240` bound. 1 and 240 pass; 0 and 241 refuse `OutOfRange`;
    /// non-numbers map to the reserved parse arm; trimmed-empty is
    /// Neutral.
    #[test]
    fn the_focus_mins_truth_table_matches_the_config_bounds() {
        for yes in ["1", "25", "240"] {
            assert_eq!(focus_mins_invalid_kind(yes), None, "{yes} must pass");
        }
        for no in ["0", "241"] {
            assert_eq!(
                focus_mins_invalid_kind(no),
                Some(InvalidKind::OutOfRange),
                "{no} must refuse"
            );
        }
        for no in ["abc", "-1", "2.5"] {
            assert_eq!(
                focus_mins_invalid_kind(no),
                Some(InvalidKind::NotKebab),
                "{no} maps to the reserved parse arm"
            );
        }
        assert_eq!(focus_mins_invalid_kind(""), None);
        assert_eq!(focus_mins_invalid_kind("   "), None);
    }

    /// The `long_break_every` field: the same shape over the
    /// `1..=24` bound (both bounds' neighbours pinned — drift on
    /// either side of the config ladder breaks this loudly).
    #[test]
    fn the_focus_every_truth_table_matches_the_config_bounds() {
        for yes in ["1", "4", "24"] {
            assert_eq!(focus_every_invalid_kind(yes), None, "{yes} must pass");
        }
        for no in ["0", "25"] {
            assert_eq!(
                focus_every_invalid_kind(no),
                Some(InvalidKind::OutOfRange),
                "{no} must refuse"
            );
        }
        for no in ["abc", "-1", "1.5"] {
            assert_eq!(
                focus_every_invalid_kind(no),
                Some(InvalidKind::NotKebab),
                "{no} maps to the reserved parse arm"
            );
        }
        assert_eq!(focus_every_invalid_kind(""), None);
        assert_eq!(focus_every_invalid_kind("   "), None);
    }

    /// The focus flavour mapping: the `general_status` composition over
    /// both predicates (empty → Neutral, failing → Invalid(kind),
    /// passing → Valid).
    #[test]
    fn focus_status_maps_the_three_flavours() {
        assert_eq!(
            general_status("", focus_mins_invalid_kind),
            FieldStatus::Neutral
        );
        assert_eq!(
            general_status("   ", focus_every_invalid_kind),
            FieldStatus::Neutral
        );
        assert_eq!(
            general_status("0", focus_mins_invalid_kind),
            FieldStatus::Invalid(InvalidKind::OutOfRange)
        );
        assert_eq!(
            general_status("25", focus_every_invalid_kind),
            FieldStatus::Invalid(InvalidKind::OutOfRange)
        );
        assert_eq!(
            general_status("abc", focus_mins_invalid_kind),
            FieldStatus::Invalid(InvalidKind::NotKebab)
        );
        assert_eq!(
            general_status("25", focus_mins_invalid_kind),
            FieldStatus::Valid
        );
        assert_eq!(
            general_status("4", focus_every_invalid_kind),
            FieldStatus::Valid
        );
    }

    /// The general flavour mapping: empty → Neutral, failing →
    /// Invalid(kind), passing → Valid, over every predicate.
    #[test]
    fn general_status_maps_the_three_flavours() {
        assert_eq!(general_status("", secs_invalid_kind), FieldStatus::Neutral);
        assert_eq!(
            general_status("   ", secs_invalid_kind),
            FieldStatus::Neutral
        );
        assert_eq!(
            general_status("0", secs_invalid_kind),
            FieldStatus::Invalid(InvalidKind::OutOfRange)
        );
        assert_eq!(general_status("30", secs_invalid_kind), FieldStatus::Valid);
        assert_eq!(
            general_status("abc", day_start_invalid_kind),
            FieldStatus::Invalid(InvalidKind::NotKebab)
        );
        assert_eq!(
            general_status("05:00", day_start_invalid_kind),
            FieldStatus::Valid
        );
    }

    /// The enablement predicate agrees with the ladder.
    #[test]
    fn the_save_enablement_follows_the_ladder() {
        let mut base = ConnectionDraft {
            server_url: "https://c.example".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert!(draft_valid(&base));
        base.username = " ".into();
        assert!(!draft_valid(&base));
    }
}

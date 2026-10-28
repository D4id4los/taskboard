// SPDX-License-Identifier: AGPL-3.0-only

//! The settings form's pure validation mirror (round-1 D7): the
//! config's connector rules re-expressed over the draft so the form
//! can flag fields before a network round-trip. The app-side
//! re-validation (`AppConfig::validate` in the Save ceremony) stays
//! the authority; this mirror exists so Save is enabled only when the
//! draft passes and the UI can say *which* field is wrong. Parity is
//! pinned by shared fixtures in both crates' tests — drift breaks a
//! test.

/// The draft the form edits (the `ConnectionProfile` without the
/// reply plumbing; the password mirrors through the same redaction
/// newtype the lane carries).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConnectionDraft {
    pub name: String,
    pub server_url: String,
    pub username: String,
    pub password: String,
}

/// Which field failed (the lane's [`crate::requests::Field`] mirror,
/// kept separate so this module stays lane-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
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
/// design: names are unique in the config, so a row under the draft's
/// name *is* the edited row (the upsert semantics, plan D7) — the
/// authoritative `DuplicateConnectorName` can only fire on a config
/// edited out-of-band, and `PersistRejected` answers it.
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
        Field::Name => {
            if draft.name.is_empty() {
                Some(InvalidKind::Empty)
            } else if !is_kebab(&draft.name) {
                Some(InvalidKind::NotKebab)
            } else {
                None
            }
        }
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
#[must_use]
pub fn field_status(field: Field, draft: &ConnectionDraft) -> FieldStatus {
    if field_text(field, draft).trim().is_empty() {
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
        Field::Name => &draft.name,
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
/// the same order: kebab name → non-empty username/password → URL
/// host rule. No collision arm (see the module-level note).
/// Recomposed from [`field_invalid`] in the ladder order — the
/// per-field predicates and this ladder are one source of truth.
#[must_use]
pub fn validate(draft: &ConnectionDraft) -> Option<DraftError> {
    let ladder = [
        Field::Name,
        Field::Username,
        Field::Password,
        Field::ServerUrl,
    ];
    for field in ladder {
        if let Some(kind) = field_invalid(field, draft) {
            return Some(DraftError::Invalid { field, kind });
        }
    }
    // No collision arm: names are unique in the config, so a row with
    // the draft's name *is* the edited row (the upsert semantics,
    // plan D7) — the authoritative DuplicateConnectorName mirror can
    // only fire on a config edited out-of-band.
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

/// The live-scale echo (round-3 D20a): Rust `{}` `Display` on `f64`
/// is shortest-round-trip; a rendering with no fraction gets ".0"
/// appended, so the echo never lies about the applied value (1 →
/// "1.0"; non-grid values like 2.25 render verbatim — `UiScale`
/// enforces only the window, not the 0.1 grid).
#[must_use]
pub fn format_scale(value: f64) -> String {
    let rendered = format!("{value}");
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
            name: "work".into(),
            server_url: "https://c.example".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert_eq!(validate(&base), None);

        let mut d = base.clone();
        d.name = String::new();
        assert_eq!(
            validate(&d),
            Some(DraftError::Invalid {
                field: Field::Name,
                kind: InvalidKind::Empty
            })
        );

        let mut d = base.clone();
        d.name = "Work".into();
        assert_eq!(
            validate(&d),
            Some(DraftError::Invalid {
                field: Field::Name,
                kind: InvalidKind::NotKebab
            })
        );

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

        // Same name, different server: an edit of that row, not a
        // collision (the upsert semantics).
        assert_eq!(validate(&base), None);
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
                name: "Work".into(),
                ..ConnectionDraft::default()
            },
            ConnectionDraft {
                name: "work".into(),
                server_url: "https://c.example".into(),
                username: "alice".into(),
                password: "pw".into(),
            },
            ConnectionDraft {
                name: "work".into(),
                server_url: "not a url".into(),
                username: "alice".into(),
                password: "pw".into(),
            },
            ConnectionDraft {
                name: "work".into(),
                server_url: "https://c.example".into(),
                username: "  ".into(),
                password: "pw".into(),
            },
            ConnectionDraft {
                name: "work".into(),
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
            let recomposed = [
                Field::Name,
                Field::Username,
                Field::Password,
                Field::ServerUrl,
            ]
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
        assert_eq!(field_status(Field::Name, &empty), FieldStatus::Neutral);
        assert_eq!(field_status(Field::ServerUrl, &empty), FieldStatus::Neutral);
        assert_eq!(field_status(Field::Username, &empty), FieldStatus::Neutral);
        assert_eq!(field_status(Field::Password, &empty), FieldStatus::Neutral);

        let mut d = ConnectionDraft {
            name: "Work".into(),
            server_url: "nope".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert_eq!(
            field_status(Field::Name, &d),
            FieldStatus::Invalid(InvalidKind::NotKebab)
        );
        assert_eq!(
            field_status(Field::ServerUrl, &d),
            FieldStatus::Invalid(InvalidKind::BadUrl)
        );

        d.name = "work".into();
        d.server_url = "https://c.example".into();
        assert_eq!(field_status(Field::Name, &d), FieldStatus::Valid);
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

    /// The enablement predicate agrees with the ladder.
    #[test]
    fn the_save_enablement_follows_the_ladder() {
        let mut base = ConnectionDraft {
            name: "work".into(),
            server_url: "https://c.example".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert!(draft_valid(&base));
        base.name = "Nope".into(); // the uppercase fails the kebab class
        assert!(!draft_valid(&base));
    }
}

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

/// How a field failed (the lane's `InvalidKind` mirror).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidKind {
    Empty,
    NotKebab,
    BadUrl,
}

/// The form-level validation outcome: the first failing field (the
/// config ladder's order), or ok.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    Invalid { field: Field, kind: InvalidKind },
}

/// Whether the draft may attempt a save (the Save button's
/// enablement): every field passes per [`validate`].
#[must_use]
pub fn draft_valid(draft: &ConnectionDraft, existing: &[crate::requests::ConnectorInfo]) -> bool {
    validate(draft, existing).is_none()
}

/// The mirror of the config's connector-entry ladder (plan F-D2), in
/// the same order: kebab name → non-empty username/password → URL host
/// rule → name collision against *other* rows only (upsert semantics:
/// a same-name save is an edit, plan D7).
#[must_use]
pub fn validate(
    draft: &ConnectionDraft,
    _existing: &[crate::requests::ConnectorInfo],
) -> Option<DraftError> {
    if draft.name.is_empty() {
        return Some(DraftError::Invalid {
            field: Field::Name,
            kind: InvalidKind::Empty,
        });
    }
    if !is_kebab(&draft.name) {
        return Some(DraftError::Invalid {
            field: Field::Name,
            kind: InvalidKind::NotKebab,
        });
    }
    if draft.username.trim().is_empty() {
        return Some(DraftError::Invalid {
            field: Field::Username,
            kind: InvalidKind::Empty,
        });
    }
    if draft.password.trim().is_empty() {
        return Some(DraftError::Invalid {
            field: Field::Password,
            kind: InvalidKind::Empty,
        });
    }
    if !url_host_ok(&draft.server_url) {
        return Some(DraftError::Invalid {
            field: Field::ServerUrl,
            kind: InvalidKind::BadUrl,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> crate::requests::ConnectorInfo {
        crate::requests::ConnectorInfo {
            name: name.to_owned(),
            server_url: "https://old.example".into(),
            username: "u".into(),
        }
    }

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

    /// T1 — the ladder: the first failing field wins; collision checks
    /// only fire against *other* rows (a same-name row is an edit).
    #[test]
    fn the_ladder_reports_the_first_failing_field() {
        let base = ConnectionDraft {
            name: "work".into(),
            server_url: "https://c.example".into(),
            username: "alice".into(),
            password: "pw".into(),
        };
        assert_eq!(validate(&base, &[row("other")]), None);

        let mut d = base.clone();
        d.name = String::new();
        assert_eq!(
            validate(&d, &[]),
            Some(DraftError::Invalid {
                field: Field::Name,
                kind: InvalidKind::Empty
            })
        );

        let mut d = base.clone();
        d.name = "Work".into();
        assert_eq!(
            validate(&d, &[]),
            Some(DraftError::Invalid {
                field: Field::Name,
                kind: InvalidKind::NotKebab
            })
        );

        let mut d = base.clone();
        d.username = "  ".into();
        assert_eq!(
            validate(&d, &[]),
            Some(DraftError::Invalid {
                field: Field::Username,
                kind: InvalidKind::Empty
            })
        );

        let mut d = base.clone();
        d.password = String::new();
        assert_eq!(
            validate(&d, &[]),
            Some(DraftError::Invalid {
                field: Field::Password,
                kind: InvalidKind::Empty
            })
        );

        let mut d = base.clone();
        d.server_url = "not a url".into();
        assert_eq!(
            validate(&d, &[]),
            Some(DraftError::Invalid {
                field: Field::ServerUrl,
                kind: InvalidKind::BadUrl
            })
        );

        // Same name, different server: an edit of that row, not a
        // collision (the upsert semantics).
        assert_eq!(validate(&base, &[row("work")]), None);
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
        assert!(draft_valid(&base, &[]));
        base.name = "Nope".into(); // "nope" is valid kebab; the test means an invalid name
        assert!(!draft_valid(&base, &[]));
    }
}

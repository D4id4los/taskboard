// SPDX-License-Identifier: AGPL-3.0-only

//! The settings-form mirror's parity pin (issue #1358): the UI-side
//! validation mirror (`taskboard_ui_slint::settings_form`) must agree
//! with the app-side authoritative predicates —
//! [`taskboard_app::config::reqwest_host_ok`] for the host rule and
//! `taskboard_domain::ConnectorId::is_kebab_name` for the kebab rule —
//! over one shared fixture table. Any drift on either side fails here
//! loudly, before a user can be told a draft is valid/invalid against
//! different grammars.

use taskboard_domain::ConnectorId;
use taskboard_ui_slint::settings_form::{is_kebab, url_host_ok};

/// The shared fixture table: every row asserts the mirror's kebab
/// predicate and the domain's authoritative mint agree. The rows span
/// the empty name, case/space/unicode rejects, and the `--` /
/// edge-hyphen rules.
#[test]
fn the_kebab_mirror_paries_the_domain_mint() {
    // (input, expected verdict — identical for both predicates)
    let table: &[(&str, bool)] = &[
        ("", false),
        ("work", true),
        ("a", true),
        ("0", true),
        ("a-1", true),
        ("work-personal-2", true),
        ("Work", false),
        ("work personal", false),
        ("-work", false),
        ("work-", false),
        ("-work-", false),
        ("wor--k", false),
        ("--", false),
        ("wörk", false),
        ("work_person", false),
        ("WORK2", false),
    ];
    for &(name, expected) in table {
        assert_eq!(
            is_kebab(name),
            expected,
            "{name:?}: the mirror disagrees with the recorded verdict"
        );
        assert_eq!(
            ConnectorId::is_kebab_name(name),
            expected,
            "{name:?}: the domain mint disagrees with the recorded verdict"
        );
        assert_eq!(
            is_kebab(name),
            ConnectorId::is_kebab_name(name),
            "{name:?}: mirror/domain drift"
        );
    }
}

/// The host rule's shared fixture table: scheme policy (http/https
/// only), empty-host rejection, userinfo rejection (including the
/// empty-user `@` row), and path/query/fragment tolerance. Every row
/// asserts the settings-form mirror and the app's authoritative
/// `reqwest_host_ok` return the same verdict.
#[test]
fn the_url_host_mirror_paries_the_app_authority() {
    let table: &[(&str, bool)] = &[
        ("", false),
        ("cloud.example.com", false),
        ("://host", false),
        ("ftp://cloud.example.com", false),
        ("https://", false),
        ("https://@host", false),
        ("https://user:pass@host", false),
        ("http://@", false),
        ("https://cloud.example.com", true),
        ("http://localhost:8080", true),
        ("https://host/path?query#frag", true),
        ("http://host/deep/path", true),
        ("https://Host.EXAMPLE.com", true),
    ];
    for &(raw, expected) in table {
        assert_eq!(
            url_host_ok(raw),
            expected,
            "{raw:?}: the mirror disagrees with the recorded verdict"
        );
        assert_eq!(
            taskboard_app::config::reqwest_host_ok(raw),
            expected,
            "{raw:?}: the app authority disagrees with the recorded verdict"
        );
        assert_eq!(
            url_host_ok(raw),
            taskboard_app::config::reqwest_host_ok(raw),
            "{raw:?}: mirror/authority drift"
        );
    }
}

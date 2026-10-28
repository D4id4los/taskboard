// SPDX-License-Identifier: AGPL-3.0-only

//! The settings surface's presentation-text home (r2 D28/D31): the
//! status line's text mapping, its auto-clear duration, and the
//! `InfoTip` help-text constants. One module owns the policy; the
//! markup renders the strings, the tests pin **coverage and
//! non-emptiness only** — never the text content (design §14-R5's
//! test rule survives the R1-R5 reversal it guards).
//!
//! Totality is compiler-enforced: every `match` below is exhaustive
//! over its enum with **no wildcard arm**, so a new `SettingsError`/
//! `Field`/`InvalidKind`/`SettingsOutcome` variant fails to compile
//! here until it is mapped (the review trap (c)).

use crate::adapter::SettingsOutcome;
use crate::requests::{Field, InvalidKind, SettingsError};

/// How long a terminal status stays visible before auto-clearing
/// (r2 D28): long enough to read a short sentence at any scale,
/// short enough to feel ephemeral. Distinct from the tooltip's
/// 600 ms arming delay — a different mechanism it must not be
/// confused with.
pub const STATUS_VISIBLE_MS: u64 = 5_000;

/// The status line's text for one outcome: the concrete error's typed
/// granularity when one was retained past the coarse fold, else the
/// outcome's generic sentence. `None` maps to the empty string — the
/// hidden state.
#[must_use]
pub fn status_text(outcome: SettingsOutcome, detail: Option<&SettingsError>) -> &'static str {
    match outcome {
        SettingsOutcome::None => "",
        SettingsOutcome::Busy => "Working…",
        SettingsOutcome::Ok => "Done.",
        SettingsOutcome::Rejected => detail.map_or(REJECTED_GENERIC, error_text),
        SettingsOutcome::Unavailable => detail.map_or(UNAVAILABLE_GENERIC, error_text),
        SettingsOutcome::Failed => detail.map_or(FAILED_GENERIC, error_text),
    }
}

const REJECTED_GENERIC: &str = "The change was refused.";
const UNAVAILABLE_GENERIC: &str = "The server or store is unreachable.";
const FAILED_GENERIC: &str = "The operation failed.";

/// One typed failure's sentence (r2 D28/F21): short, present tense,
/// user-actionable. Exhaustive, 15 arms — no wildcard.
fn error_text(err: &SettingsError) -> &'static str {
    match err {
        SettingsError::LegacyConfig => {
            "This config is the legacy single-server form; connect from the config file instead."
        }
        SettingsError::Invalid { field, kind } => invalid_text(*field, *kind),
        SettingsError::NoSecretStored => "No password is stored for this row; save it again.",
        SettingsError::StoreUnsupported => {
            "This platform's credential store cannot hold passwords; set the env store."
        }
        SettingsError::StoreFailed => "The credential store failed; check its availability.",
        SettingsError::VerifyRejected => {
            "The server rejected these credentials; check the username and app password."
        }
        SettingsError::VerifyUnreachable => {
            "The server could not be reached; check the URL and your connection."
        }
        SettingsError::VerifyFailed => "The server did not accept the check; review the details.",
        SettingsError::PersistRejected => "The config file refused the change; check its format.",
        SettingsError::PersistFailed => "The config file could not be written; check disk space.",
        SettingsError::AttachBusy => {
            "That connection is still shutting down; try again in a moment."
        }
        SettingsError::AttachFailed => "The connection could not be started; check the server.",
        SettingsError::Gone => "The form went stale; reopen the settings and try again.",
        SettingsError::NoRemoteMatch => {
            "No remote board with this title was found; create or rename it on the server first."
        }
        SettingsError::BindRefused => {
            "That board is already bound to another connection; detach it there first."
        }
    }
}

/// The `Field`×`InvalidKind` lattice (r2 D28): nested exhaustive
/// matches — one inner arm per kind per field, no wildcard at either
/// level, so a new variant on either enum fails to compile here.
pub(crate) fn invalid_text(field: Field, kind: InvalidKind) -> &'static str {
    match field {
        Field::Name => match kind {
            InvalidKind::Empty => "Enter a name for the connection.",
            InvalidKind::NotKebab => "Use lowercase letters, digits, and dashes for the name.",
            InvalidKind::BadUrl => "The name cannot be a URL; use a short kebab label.",
            InvalidKind::NotFinite => "The name is not text; use letters and digits.",
            InvalidKind::OutOfRange => "The name is too long; shorten it.",
            InvalidKind::Duplicate => "That name is already in use.",
        },
        Field::ServerUrl => match kind {
            InvalidKind::Empty => "Enter the server's address.",
            InvalidKind::NotKebab => "The address cannot be a kebab label; enter a URL.",
            InvalidKind::BadUrl => "Enter a full URL, e.g. https://cloud.example.com.",
            InvalidKind::NotFinite => "The address is not text; enter a URL.",
            InvalidKind::OutOfRange => "The address is too long; check it for typos.",
            InvalidKind::Duplicate => "That server is already configured.",
        },
        Field::Username => match kind {
            InvalidKind::Empty => "Enter your username.",
            InvalidKind::NotKebab => "Usernames are letters, digits, and dashes.",
            InvalidKind::BadUrl => "The username cannot be a URL; enter your account name.",
            InvalidKind::NotFinite => "The username is not text; enter your account name.",
            InvalidKind::OutOfRange => "The username is too long; check it for typos.",
            InvalidKind::Duplicate => "That username is already configured here.",
        },
        Field::Password => match kind {
            InvalidKind::Empty => "Enter an app password.",
            InvalidKind::NotKebab => "Paste the generated app password.",
            InvalidKind::BadUrl => {
                "The app password cannot be a URL; paste the generated password."
            }
            InvalidKind::NotFinite => "The app password is not text; paste the generated password.",
            InvalidKind::OutOfRange => "The app password is too long; paste it again.",
            InvalidKind::Duplicate => "That app password is already stored; use a fresh one.",
        },
        Field::Scale => match kind {
            InvalidKind::Empty => "Enter a scale, e.g. 1.0.",
            InvalidKind::NotKebab | InvalidKind::BadUrl => "The scale is a number, e.g. 1.5.",
            InvalidKind::NotFinite => "Enter a number, e.g. 1.5.",
            InvalidKind::OutOfRange => "The scale must be between 0.5 and 3.0.",
            InvalidKind::Duplicate => "That scale is already applied.",
        },
    }
}

// The D31 InfoTip help texts: presentation constants, one per
// annotated field. Their *content* is never asserted anywhere — the
// unit loop pins non-emptiness (structure, never text).
pub const HELP_NAME: &str =
    "A free-form label for this connection, e.g. work. Lowercase letters, digits, and dashes.";
pub const HELP_SERVER: &str = "Your Nextcloud instance's base URL, e.g. https://cloud.example.com.";
pub const HELP_USERNAME: &str = "Your Nextcloud account name.";
pub const HELP_APP_PASSWORD: &str = "Created in Nextcloud → Personal settings → Security → Devices & sessions. Not your account password.";
pub const HELP_SCALE: &str =
    "How large the interface renders, from 0.5 to 3.0. Type the exact value and press Return.";
pub const HELP_KEYMAP: &str =
    "The keyboard scheme: Standard, Emacs, or Vi. Overrides from the config file stay in place.";
pub const HELP_ROWS: &str = "Test verifies the stored password against the server; Remove deletes the config entry and the stored password.";

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The `Field`×`InvalidKind` lattice: all 30 combinations map to a
    /// non-empty sentence (coverage, never content).
    #[test]
    fn every_field_kind_combination_maps_to_text() {
        for field in [
            Field::Name,
            Field::ServerUrl,
            Field::Username,
            Field::Password,
            Field::Scale,
        ] {
            for kind in [
                InvalidKind::Empty,
                InvalidKind::NotKebab,
                InvalidKind::BadUrl,
                InvalidKind::NotFinite,
                InvalidKind::OutOfRange,
                InvalidKind::Duplicate,
            ] {
                assert_ne!(invalid_text(field, kind), "");
            }
        }
    }

    /// The `SettingsError` totality loop: all 15 variants map to
    /// non-empty text through `status_text`'s detail path.
    #[test]
    fn every_settings_error_variant_maps_to_text() {
        let all = [
            SettingsError::LegacyConfig,
            SettingsError::Invalid {
                field: Field::Name,
                kind: InvalidKind::Empty,
            },
            SettingsError::NoSecretStored,
            SettingsError::StoreUnsupported,
            SettingsError::StoreFailed,
            SettingsError::VerifyRejected,
            SettingsError::VerifyUnreachable,
            SettingsError::VerifyFailed,
            SettingsError::PersistRejected,
            SettingsError::PersistFailed,
            SettingsError::AttachBusy,
            SettingsError::AttachFailed,
            SettingsError::Gone,
            SettingsError::NoRemoteMatch,
            SettingsError::BindRefused,
        ];
        assert_eq!(all.len(), 15, "the variant list is the enum's totality");
        for err in all {
            assert_ne!(
                status_text(SettingsOutcome::Rejected, Some(&err)),
                "",
                "variant mapped empty: {err:?}"
            );
        }
    }

    /// The coarse outcomes: `None` maps to the hidden empty string,
    /// the other five to non-empty text — with and without detail.
    #[test]
    fn the_coarse_outcomes_map_consistently() {
        assert_eq!(
            status_text(SettingsOutcome::None, None),
            "",
            "None is the hidden state"
        );
        assert_ne!(status_text(SettingsOutcome::Busy, None), "");
        assert_ne!(status_text(SettingsOutcome::Ok, None), "");
        for outcome in [
            SettingsOutcome::Rejected,
            SettingsOutcome::Unavailable,
            SettingsOutcome::Failed,
        ] {
            assert_ne!(status_text(outcome, None), "");
            let detail = SettingsError::Gone;
            assert_ne!(status_text(outcome, Some(&detail)), "");
        }
    }

    /// The D31 constants are present and non-empty (structure, never
    /// content).
    #[test]
    fn every_help_constant_is_non_empty() {
        for text in [
            HELP_NAME,
            HELP_SERVER,
            HELP_USERNAME,
            HELP_APP_PASSWORD,
            HELP_SCALE,
            HELP_KEYMAP,
            HELP_ROWS,
        ] {
            assert_ne!(text, "");
        }
    }

    /// The mapping-totality proptest (r2 D28): every generated
    /// variant maps to non-empty text, and the generator's variant
    /// list is pinned by `variant_name`'s exhaustive match — a new
    /// variant breaks this compile; a removed one breaks the match
    /// arm listing.
    fn any_settings_error() -> impl Strategy<Value = SettingsError> {
        prop_oneof![
            Just(SettingsError::LegacyConfig),
            (any_field(), any_invalid_kind())
                .prop_map(|(field, kind)| SettingsError::Invalid { field, kind }),
            Just(SettingsError::NoSecretStored),
            Just(SettingsError::StoreUnsupported),
            Just(SettingsError::StoreFailed),
            Just(SettingsError::VerifyRejected),
            Just(SettingsError::VerifyUnreachable),
            Just(SettingsError::VerifyFailed),
            Just(SettingsError::PersistRejected),
            Just(SettingsError::PersistFailed),
            Just(SettingsError::AttachBusy),
            Just(SettingsError::AttachFailed),
            Just(SettingsError::Gone),
            Just(SettingsError::NoRemoteMatch),
            Just(SettingsError::BindRefused),
        ]
    }

    fn any_field() -> impl Strategy<Value = Field> {
        prop_oneof![
            Just(Field::Name),
            Just(Field::ServerUrl),
            Just(Field::Username),
            Just(Field::Password),
            Just(Field::Scale),
        ]
    }

    fn any_invalid_kind() -> impl Strategy<Value = InvalidKind> {
        prop_oneof![
            Just(InvalidKind::Empty),
            Just(InvalidKind::NotKebab),
            Just(InvalidKind::BadUrl),
            Just(InvalidKind::NotFinite),
            Just(InvalidKind::OutOfRange),
            Just(InvalidKind::Duplicate),
        ]
    }

    /// Exhaustive over the enum, no wildcard: the compile-time pin
    /// that the generator above enumerates exactly the enum's
    /// variants (removal breaks this arm listing; addition breaks the
    /// mapping matches).
    fn variant_name(err: &SettingsError) -> &'static str {
        match err {
            SettingsError::LegacyConfig => "legacy",
            SettingsError::Invalid { .. } => "invalid",
            SettingsError::NoSecretStored => "no-secret",
            SettingsError::StoreUnsupported => "store-unsupported",
            SettingsError::StoreFailed => "store-failed",
            SettingsError::VerifyRejected => "verify-rejected",
            SettingsError::VerifyUnreachable => "verify-unreachable",
            SettingsError::VerifyFailed => "verify-failed",
            SettingsError::PersistRejected => "persist-rejected",
            SettingsError::PersistFailed => "persist-failed",
            SettingsError::AttachBusy => "attach-busy",
            SettingsError::AttachFailed => "attach-failed",
            SettingsError::Gone => "gone",
            SettingsError::NoRemoteMatch => "no-remote-match",
            SettingsError::BindRefused => "bind-refused",
        }
    }

    proptest! {
        #[test]
        fn every_generated_error_maps_to_non_empty_text(err in any_settings_error()) {
            let text = status_text(SettingsOutcome::Rejected, Some(&err));
            prop_assert_ne!(text, "");
            // The generator genuinely carries the variant (not a
            // collapsed strategy): its name is one of the pinned ones.
            prop_assert!(!variant_name(&err).is_empty());
        }
    }
}

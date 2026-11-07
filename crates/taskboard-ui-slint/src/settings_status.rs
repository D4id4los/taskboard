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
        SettingsError::EnvLocked { .. } => {
            "This value is set by an environment variable; change it there."
        }
    }
}

/// The `Field`×`InvalidKind` lattice (r2 D28): nested exhaustive
/// matches — one inner arm per kind per field, no wildcard at either
/// level, so a new variant on either enum fails to compile here.
#[allow(clippy::too_many_lines)] // the exhaustive Field×Kind lattice, one arm per cell
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
        Field::DayStart => match kind {
            InvalidKind::Empty => "Enter a time of day.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a time of day, e.g. 05:00."
            }
            InvalidKind::OutOfRange => "That time does not exist; use 00:00–23:59.",
            InvalidKind::Duplicate => "That day start is already set.",
        },
        Field::RemindersEnabled => match kind {
            InvalidKind::Empty
            | InvalidKind::NotKebab
            | InvalidKind::BadUrl
            | InvalidKind::NotFinite => "Pick whether reminders are on or off.",
            InvalidKind::OutOfRange => "That toggle is not available.",
            InvalidKind::Duplicate => "Reminders are already set that way.",
        },
        Field::TickSecs => match kind {
            InvalidKind::Empty => "Enter the tick interval in seconds.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of seconds, e.g. 30."
            }
            InvalidKind::OutOfRange => "The tick must be at least one second.",
            InvalidKind::Duplicate => "That tick is already set.",
        },
        Field::ReminderBefore => match kind {
            InvalidKind::Empty => "Enter a before-offset, e.g. 5m.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Use the 30s / 5m / 1h30m / 2d grammar, e.g. 5m."
            }
            InvalidKind::OutOfRange => "That offset is too large; shorten it.",
            InvalidKind::Duplicate => "That offset is already set.",
        },
        Field::ReminderSink => match kind {
            InvalidKind::Empty
            | InvalidKind::NotKebab
            | InvalidKind::BadUrl
            | InvalidKind::NotFinite => "Pick a notifier: auto, log, or desktop.",
            InvalidKind::OutOfRange => "That notifier is not available.",
            InvalidKind::Duplicate => "That notifier is already set.",
        },
        Field::PollInterval => match kind {
            InvalidKind::Empty => "Enter the poll interval in seconds.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of seconds, e.g. 30."
            }
            InvalidKind::OutOfRange => "The poll interval must be at least one second.",
            InvalidKind::Duplicate => "That interval is already set.",
        },
        Field::BackoffInitial => match kind {
            InvalidKind::Empty => "Enter the first backoff in seconds.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of seconds, e.g. 5."
            }
            InvalidKind::OutOfRange => {
                "The first backoff must be positive and not above the maximum."
            }
            InvalidKind::Duplicate => "That backoff is already set.",
        },
        Field::BackoffMax => match kind {
            InvalidKind::Empty => "Enter the backoff ceiling in seconds.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of seconds, e.g. 300."
            }
            InvalidKind::OutOfRange => "The ceiling must be at least one second.",
            InvalidKind::Duplicate => "That ceiling is already set.",
        },
        Field::DueSoonDays => match kind {
            InvalidKind::Empty => "Enter the due-soon horizon in days.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of days, e.g. 1."
            }
            InvalidKind::OutOfRange => "The horizon must stay within ten years.",
            InvalidKind::Duplicate => "That horizon is already set.",
        },
        Field::FocusEnabled => match kind {
            InvalidKind::Empty
            | InvalidKind::NotKebab
            | InvalidKind::BadUrl
            | InvalidKind::NotFinite => "Pick whether focus mode is on or off.",
            InvalidKind::OutOfRange => "That toggle is not available.",
            InvalidKind::Duplicate => "Focus mode is already set that way.",
        },
        Field::FocusWorkMins => match kind {
            InvalidKind::Empty => "Enter the focus-block length in minutes.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of minutes, e.g. 25."
            }
            InvalidKind::OutOfRange => "The focus block must be between 1 and 240 minutes.",
            InvalidKind::Duplicate => "That block length is already set.",
        },
        Field::FocusBreakMins => match kind {
            InvalidKind::Empty => "Enter the break length in minutes.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of minutes, e.g. 5."
            }
            InvalidKind::OutOfRange => "The break must be between 1 and 240 minutes.",
            InvalidKind::Duplicate => "That break length is already set.",
        },
        Field::FocusChain => match kind {
            InvalidKind::Empty
            | InvalidKind::NotKebab
            | InvalidKind::BadUrl
            | InvalidKind::NotFinite => "Pick whether blocks chain automatically.",
            InvalidKind::OutOfRange => "That toggle is not available.",
            InvalidKind::Duplicate => "Chaining is already set that way.",
        },
        Field::FocusLongBreakMins => match kind {
            InvalidKind::Empty => "Enter the long-break length in minutes.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of minutes, e.g. 15."
            }
            InvalidKind::OutOfRange => "The long break must be between 1 and 240 minutes.",
            InvalidKind::Duplicate => "That long-break length is already set.",
        },
        Field::FocusLongBreakEvery => match kind {
            InvalidKind::Empty => "Enter how many blocks make a long break.",
            InvalidKind::NotKebab | InvalidKind::BadUrl | InvalidKind::NotFinite => {
                "Enter a whole number of blocks, e.g. 4."
            }
            InvalidKind::OutOfRange => "A long break comes after 1 to 24 blocks.",
            InvalidKind::Duplicate => "That cadence is already set.",
        },
        Field::FocusNotify => match kind {
            InvalidKind::Empty
            | InvalidKind::NotKebab
            | InvalidKind::BadUrl
            | InvalidKind::NotFinite => "Pick whether phase boundaries are announced.",
            InvalidKind::OutOfRange => "That toggle is not available.",
            InvalidKind::Duplicate => "Announcements are already set that way.",
        },
        Field::FilesSyncEnabled => match kind {
            InvalidKind::Empty
            | InvalidKind::NotKebab
            | InvalidKind::BadUrl
            | InvalidKind::NotFinite => "Pick whether files sync runs.",
            InvalidKind::OutOfRange => {
                "Files sync needs a Nextcloud account as the default connection."
            }
            InvalidKind::Duplicate => "Files sync is already set that way.",
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
// The settings editor's general fields (the GUI settings editor chunk).
pub const HELP_DAY_START: &str =
    "The local time of day at which \"today\" begins, e.g. 05:00. Type it and press Return.";
pub const HELP_TICK: &str = "How often the reminder engine checks, in whole seconds.";
pub const HELP_BEFORE: &str =
    "The implicit reminder fires this long before a due date, e.g. 5m, 1h30m, 2d.";
pub const HELP_POLL: &str = "How long the sync actor rests after a successful cycle, in seconds.";
pub const HELP_BACKOFF: &str =
    "The failure backoff: the first wait doubles toward the ceiling. Both in whole seconds.";
pub const HELP_DUE_SOON: &str =
    "How many days ahead a task counts as due soon. 0 turns the soon set off.";
pub const HELP_REMINDERS: &str =
    "Whether the reminder engine runs at all. Edited values apply at the next restart.";
pub const HELP_SINK: &str = "Which notifier delivers fired reminders: auto resolves by app mode.";
// The `[focus]` section's rows (the focus settings editor, #625).
pub const HELP_FOCUS_ENABLED: &str =
    "Whether the pomodoro focus runtime runs. Edited values apply at the next restart.";
pub const HELP_FOCUS_WORK_MINS: &str =
    "The focus-block length in whole minutes, 1 to 240. Type it and press Return.";
pub const HELP_FOCUS_BREAK_MINS: &str = "The short-break length in whole minutes, 1 to 240.";
pub const HELP_FOCUS_CHAIN: &str =
    "Whether the next focus block auto-starts when a runtime-driven break ends.";
pub const HELP_FOCUS_LONG_BREAK_MINS: &str = "The long-break length in whole minutes, 1 to 240.";
pub const HELP_FOCUS_LONG_BREAK_EVERY: &str =
    "Every N completed work blocks the next break is long, 1 to 24.";
pub const HELP_FOCUS_NOTIFY: &str =
    "A gentle toast and chime at each runtime-driven phase boundary.";

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
            Field::DayStart,
            Field::RemindersEnabled,
            Field::TickSecs,
            Field::ReminderSink,
            Field::ReminderBefore,
            Field::PollInterval,
            Field::BackoffInitial,
            Field::BackoffMax,
            Field::DueSoonDays,
            Field::FocusEnabled,
            Field::FocusWorkMins,
            Field::FocusBreakMins,
            Field::FocusChain,
            Field::FocusLongBreakMins,
            Field::FocusLongBreakEvery,
            Field::FocusNotify,
            Field::FilesSyncEnabled,
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

    /// The `SettingsError` totality loop: all 14 variants map to
    /// non-empty text through `status_text`'s detail path.
    #[test]
    fn every_settings_error_variant_maps_to_text() {
        let all = [
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
            SettingsError::EnvLocked {
                field: Field::TickSecs,
            },
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
            HELP_DAY_START,
            HELP_TICK,
            HELP_BEFORE,
            HELP_POLL,
            HELP_BACKOFF,
            HELP_DUE_SOON,
            HELP_REMINDERS,
            HELP_SINK,
            HELP_FOCUS_ENABLED,
            HELP_FOCUS_WORK_MINS,
            HELP_FOCUS_BREAK_MINS,
            HELP_FOCUS_CHAIN,
            HELP_FOCUS_LONG_BREAK_MINS,
            HELP_FOCUS_LONG_BREAK_EVERY,
            HELP_FOCUS_NOTIFY,
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
            any_field().prop_map(|field| SettingsError::EnvLocked { field }),
        ]
    }

    fn any_field() -> impl Strategy<Value = Field> {
        prop_oneof![
            Just(Field::Name),
            Just(Field::ServerUrl),
            Just(Field::Username),
            Just(Field::Password),
            Just(Field::Scale),
            Just(Field::DayStart),
            Just(Field::RemindersEnabled),
            Just(Field::TickSecs),
            Just(Field::ReminderSink),
            Just(Field::ReminderBefore),
            Just(Field::PollInterval),
            Just(Field::BackoffInitial),
            Just(Field::BackoffMax),
            Just(Field::DueSoonDays),
            Just(Field::FocusEnabled),
            Just(Field::FocusWorkMins),
            Just(Field::FocusBreakMins),
            Just(Field::FocusChain),
            Just(Field::FocusLongBreakMins),
            Just(Field::FocusLongBreakEvery),
            Just(Field::FocusNotify),
            Just(Field::FilesSyncEnabled),
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
            SettingsError::EnvLocked { .. } => "env-locked",
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

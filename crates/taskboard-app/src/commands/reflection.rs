// SPDX-License-Identifier: AGPL-3.0-only
//! The `taskboard journal` verb family (DR-7 D-J11) over the
//! in-process core — the timers two-lane recipe verbatim: one-shot
//! wrappers boot the core over [`crate::commands::tasks::with_core`],
//! the `*_on` seams run on a booted session core, and `collapse` is
//! the one definition site both lanes share. The user-facing verb is
//! `journal`; every code symbol is `Reflection*` (the D-J1 vocabulary
//! ruling). Device-local: no sync lane anywhere.
//!
//! Every receipt reads the entry back from the published snapshot —
//! never an echo of the submitted input.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};

use taskboard_domain::reflection::{HealthFlag, ReviewCadence, review_window};
use taskboard_domain::{AppState, CommandOutcome, DayCalendar, ReflectionId, StateCommand};

use crate::commands::CommandError;
use crate::commands::output::{
    ReflectionAddReceipt, ReflectionDeleteReceipt, ReflectionEditReceipt, ReflectionRow,
    ReflectionsList,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Boots the core and records one entry (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn add(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    input: ReflectionInvocation,
) -> Result<ReflectionAddReceipt, CommandError> {
    let calendar = config.resolved_day_calendar();
    crate::commands::tasks::with_core(config, store, async |app| match input {
        ReflectionInvocation::Add {
            cadence,
            day,
            mood,
            energy,
            flags,
            text,
        } => add_on(app, calendar, cadence, day, mood, energy, flags, text).await,
        _ => unreachable!("the collapse product names add"),
    })
    .await
}

/// Boots the core and lists live entries.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    filters: ListFilters,
) -> Result<ReflectionsList, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| Ok(list_on(app, &filters))).await
}

/// Boots the core and shows one entry.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn show(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    selector: String,
) -> Result<ReflectionRow, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| show_on(app, &selector)).await
}

/// Boots the core and edits one entry.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn edit(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    selector: String,
    mask: EditMask,
) -> Result<ReflectionEditReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| {
        edit_on(app, &selector, mask).await
    })
    .await
}

/// Boots the core and tombstones one entry.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn delete(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    selector: String,
) -> Result<ReflectionDeleteReceipt, CommandError> {
    crate::commands::tasks::with_core(config, store, async |app| delete_on(app, &selector).await)
        .await
}

/// The `journal add` steps on a booted core (the session seam): derive
/// the window app-side (the D-F6/D-J11 split — the app layer owns
/// calendars, the planner stays blind), execute the record, and read
/// the entry back from the published snapshot.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the post-execute row read: the record outcome
/// names the minted id, so the same-state projection cannot miss.
#[allow(clippy::too_many_arguments)] // the payload's shape
pub async fn add_on(
    app: &crate::App,
    calendar: DayCalendar,
    cadence: ReviewCadence,
    day: Option<NaiveDate>,
    mood: Option<u8>,
    energy: Option<u8>,
    flags: BTreeSet<HealthFlag>,
    text: String,
) -> Result<ReflectionAddReceipt, CommandError> {
    // `--day` defaults to the calendar's today at the seam (Q-3: the
    // wall clock stays out of `collapse`).
    let reference = day.unwrap_or_else(|| calendar.today(Utc::now()));
    let window = review_window(cadence, reference, calendar);
    let outcome = app
        .engine
        .execute(StateCommand::RecordReflection {
            cadence,
            day: window.day(),
            window: window.bounds(),
            text,
            mood,
            energy,
            flags,
        })
        .await?;
    match outcome {
        CommandOutcome::ReflectionRecorded { entry } => {
            let state = app.engine.shared_state().load_full();
            let stored = state
                .reflection
                .entries
                .get(&entry.id)
                .expect("the record outcome names the minted entry");
            Ok(ReflectionAddReceipt {
                created: true,
                entry: Some(reflection_row(stored)),
            })
        }
        // The empty record (no text ∧ no attributes): the accepted
        // no-op is success, not an error (Q-6).
        CommandOutcome::NoOp => Ok(ReflectionAddReceipt {
            created: false,
            entry: None,
        }),
        _ => unreachable!("the record planner answers recorded or the empty no-op"),
    }
}

/// The `journal edit` steps on a booted core: resolve, execute the
/// change mask, project the row **post-execute** from the snapshot.
/// The unchanged-mask (or engine-side tombstoned-target) no-op carries
/// the current row.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the projection reads: resolution guarantees
/// the entry exists in the same snapshots they read.
pub async fn edit_on(
    app: &crate::App,
    selector: &str,
    mask: EditMask,
) -> Result<ReflectionEditReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_reflection_entry(&state, selector)?;
    let EditMask {
        text,
        mood,
        energy,
        flags_add,
        flags_remove,
    } = mask;
    let outcome = app
        .engine
        .execute(StateCommand::UpdateReflection {
            id,
            text,
            mood,
            energy,
            flags_add,
            flags_remove,
        })
        .await?;
    match outcome {
        CommandOutcome::ReflectionUpdated { .. } => {
            let state = app.engine.shared_state().load_full();
            let stored = state
                .reflection
                .entries
                .get(&id)
                .expect("the edit outcome names the edited entry");
            Ok(ReflectionEditReceipt {
                changed: true,
                entry: reflection_row(stored),
            })
        }
        CommandOutcome::NoOp => {
            let stored = state
                .reflection
                .entries
                .get(&id)
                .expect("resolution guarantees the entry exists");
            Ok(ReflectionEditReceipt {
                changed: false,
                entry: reflection_row(stored),
            })
        }
        _ => unreachable!("the edit planner answers updated or the unchanged no-op"),
    }
}

/// The `journal delete` steps on a booted core: resolve, project the
/// **pre-execute** row (post-execute the row is tombstoned — the
/// `timers cancel_on` discipline), execute the tombstone.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the pre-execute row read: resolution
/// guarantees the entry exists in the same state snapshot the
/// projection reads.
pub async fn delete_on(
    app: &crate::App,
    selector: &str,
) -> Result<ReflectionDeleteReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_reflection_entry(&state, selector)?;
    let entry = reflection_row(
        state
            .reflection
            .entries
            .get(&id)
            .expect("resolution guarantees the entry exists"),
    );
    drop(state);
    app.engine
        .execute(StateCommand::DeleteReflection { id })
        .await?;
    Ok(ReflectionDeleteReceipt {
        entry_id: id.as_uuid().to_string(),
        entry,
    })
}

/// The `journal list` steps: a pure read over the published snapshot's
/// live entries, filtered, canonical `(window_start, id)` order. No
/// command executes (the `timers::list_on` shape).
#[must_use]
pub fn list_on(app: &crate::App, filters: &ListFilters) -> ReflectionsList {
    let state = app.engine.shared_state().load_full();
    ReflectionsList {
        entries: filtered_rows(&state, filters),
    }
}

/// The `journal show` steps: the one live entry's row (the same
/// projection `list` uses — no second projection).
///
/// # Errors
///
/// [`CommandError::UnknownReflectionEntry`] / [`CommandError::AmbiguousReflectionEntry`]
/// — typed, never text-matched in the engine.
///
/// # Panics
///
/// Only via `expect` on the row read: resolution guarantees the entry
/// exists in the same state snapshot the projection reads.
pub fn show_on(app: &crate::App, selector: &str) -> Result<ReflectionRow, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_reflection_entry(&state, selector)?;
    Ok(reflection_row(
        state
            .reflection
            .entries
            .get(&id)
            .expect("resolution guarantees the entry exists"),
    ))
}

/// Resolves an entry selector against the *live* entries (the
/// `resolve_time_entry` shape minus the label pass — entries carry no
/// label): exact uuid → unique ≥8-char lowercased uuid prefix → typed
/// miss. Resolution runs over `live_entries()` only: a tombstoned
/// entry is gone from the family's own recovery surface, so its
/// selector is a typed unknown (Q-7) — an `edit`/`delete` on it never
/// reaches the engine's tombstoned no-op (that planner law stays
/// engine-tested).
///
/// # Errors
///
/// [`CommandError::UnknownReflectionEntry`] / [`CommandError::AmbiguousReflectionEntry`]
/// — typed.
pub fn resolve_reflection_entry(
    state: &AppState,
    selector: &str,
) -> Result<ReflectionId, CommandError> {
    let entries = &state.reflection.entries;
    let is_live = |id: &ReflectionId| {
        entries
            .get(id)
            .is_some_and(taskboard_domain::ReflectionEntry::is_live)
    };
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = ReflectionId::from_uuid(uuid);
        if is_live(&id) {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<ReflectionId> = entries
            .keys()
            .filter(|id| is_live(id) && id.as_uuid().to_string().starts_with(&needle))
            .copied()
            .collect();
        match hits.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            [_, ..] => {
                return Err(CommandError::AmbiguousReflectionEntry {
                    candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
                });
            }
        }
    }
    Err(CommandError::UnknownReflectionEntry {
        selector: selector.to_owned(),
    })
}

/// Projects an entity onto its row (state truth; the whole entry,
/// parked names included).
#[must_use]
pub fn reflection_row(entry: &taskboard_domain::ReflectionEntry) -> ReflectionRow {
    ReflectionRow {
        entry_id: entry.id.as_uuid().to_string(),
        cadence: entry.cadence.text_form().to_owned(),
        day: entry.day,
        window_start: entry.window_start,
        window_end: entry.window_end,
        text: entry.text.clone(),
        mood: entry
            .attributes
            .mood
            .map(taskboard_domain::reflection::Rating::get),
        energy: entry
            .attributes
            .energy
            .map(taskboard_domain::reflection::Rating::get),
        flags: entry
            .attributes
            .flags
            .iter()
            .map(|f| f.text_form().to_owned())
            .collect(),
        extra_flags: entry.attributes.extra_flags.iter().cloned().collect(),
        created_at: entry.created_at,
        updated_at: entry.updated_at,
    }
}

/// The `journal list` filters (`--cadence/--since/--until`).
#[derive(Debug, Clone, Default)]
pub struct ListFilters {
    /// Only this cadence.
    pub cadence: Option<ReviewCadence>,
    /// Entries whose window opens on/after this UTC day (inclusive).
    pub since: Option<NaiveDate>,
    /// Entries whose window opens before the day after this UTC day.
    pub until: Option<NaiveDate>,
}

/// The live rows under `filters`, canonical `(window_start, id)`
/// ascending (Q-8 — the `track list` precedent generalized to the
/// review window).
///
/// # Panics
///
/// Only via `expect` on the day successor — unreachable for any
/// representable filter date (the `day_start` comment).
#[must_use]
pub fn filtered_rows(state: &AppState, filters: &ListFilters) -> Vec<ReflectionRow> {
    let mut rows: Vec<&taskboard_domain::ReflectionEntry> = state
        .reflection
        .live_entries()
        .filter(|entry| filters.cadence.is_none_or(|c| entry.cadence == c))
        .filter(|entry| {
            filters
                .since
                .is_none_or(|since| entry.window_start >= super::track::day_start(since))
        })
        .filter(|entry| {
            filters.until.is_none_or(|until| {
                let next = until.succ_opt().expect("tomorrow exists");
                entry.window_start < super::track::day_start(next)
            })
        })
        .collect();
    rows.sort_by_key(|entry| (entry.window_start, entry.id));
    rows.iter().map(|entry| reflection_row(entry)).collect()
}

/// One resolved `journal` invocation: the clap subcommand collapsed to
/// exactly one mode.
#[derive(Debug, Clone)]
pub enum ReflectionInvocation {
    /// `add [flags] [TEXT…]`.
    Add {
        /// The reviewed window's cadence.
        cadence: ReviewCadence,
        /// The reference date (`None` = the seam's today default).
        day: Option<NaiveDate>,
        /// Mood mint input.
        mood: Option<u8>,
        /// Energy mint input.
        energy: Option<u8>,
        /// The curated flags (set semantics — repeated `--flag` dedups).
        flags: BTreeSet<HealthFlag>,
        /// The joined body text.
        text: String,
    },
    /// `list` (pure read).
    List {
        /// The filters.
        filters: ListFilters,
    },
    /// `show <selector>`.
    Show(String),
    /// `edit <selector> [mask]`.
    Edit {
        /// The entry selector.
        selector: String,
        /// The change mask.
        mask: EditMask,
    },
    /// `delete <selector>`.
    Delete(String),
}

/// The `journal edit` change mask (`None` = unchanged; ratings are
/// double-Options — `Some(None)` = clear).
#[derive(Debug, Clone, Default)]
pub struct EditMask {
    /// The new text (`Some("")` = the legal clear).
    pub text: Option<String>,
    /// The mood change mask.
    pub mood: Option<Option<u8>>,
    /// The energy change mask.
    pub energy: Option<Option<u8>>,
    /// Flags to add (the delta's positive half).
    pub flags_add: BTreeSet<HealthFlag>,
    /// Flags to remove (the delta's negative half).
    pub flags_remove: BTreeSet<HealthFlag>,
}

/// Collapses the clap subcommand into exactly one invocation. One
/// definition site for both dispatch lanes; total (the `timers`
/// shape's unreachable branch never fires here).
#[must_use]
pub fn collapse(cmd: crate::cli::ReflectionCmd) -> Option<ReflectionInvocation> {
    match cmd {
        crate::cli::ReflectionCmd::Add {
            cadence,
            day,
            mood,
            energy,
            flag,
            text_words,
        } => Some(ReflectionInvocation::Add {
            cadence: cadence.map_or(ReviewCadence::Day, crate::cli::CadenceSpec::cadence),
            day,
            mood,
            energy,
            flags: flag.into_iter().collect(),
            text: text_words.join(" "),
        }),
        crate::cli::ReflectionCmd::List {
            cadence,
            since,
            until,
        } => Some(ReflectionInvocation::List {
            filters: ListFilters {
                cadence: cadence.map(crate::cli::CadenceSpec::cadence),
                since,
                until,
            },
        }),
        crate::cli::ReflectionCmd::Show { selector } => Some(ReflectionInvocation::Show(selector)),
        crate::cli::ReflectionCmd::Edit {
            selector,
            text,
            clear_text,
            mood,
            clear_mood,
            energy,
            clear_energy,
            flag,
            unflag,
        } => Some(ReflectionInvocation::Edit {
            selector,
            mask: EditMask {
                // `--clear-text` is the payload's legal empty-text
                // clear; clap keeps it disjoint from `--text`.
                text: text.or_else(|| clear_text.then(String::new)),
                mood: mood.map(Some).or(clear_mood.then_some(None)),
                energy: energy.map(Some).or(clear_energy.then_some(None)),
                flags_add: flag.into_iter().collect(),
                flags_remove: unflag.into_iter().collect(),
            },
        }),
        crate::cli::ReflectionCmd::Delete { selector } => {
            Some(ReflectionInvocation::Delete(selector))
        }
    }
}

/// The human-readable `add` receipt (the stamped row, not an echo).
#[must_use]
pub fn render_text_add(receipt: &ReflectionAddReceipt) -> String {
    let Some(entry) = receipt.entry.as_ref() else {
        return "nothing recorded (empty entry)".to_owned();
    };
    format!("recorded {}", render_row(entry))
}

/// The human-readable `edit` receipt.
#[must_use]
pub fn render_text_edit(receipt: &ReflectionEditReceipt) -> String {
    format!(
        "changed: {} — {}",
        receipt.changed,
        render_row(&receipt.entry)
    )
}

/// The human-readable `delete` receipt (the pre-state row).
#[must_use]
pub fn render_text_delete(receipt: &ReflectionDeleteReceipt) -> String {
    format!(
        "deleted {} — {}",
        receipt.entry_id,
        render_row(&receipt.entry)
    )
}

/// The human-readable `list` rendering: one line per row in canonical
/// order; the empty family renders its own line.
#[must_use]
pub fn render_text_list(list: &ReflectionsList) -> String {
    if list.entries.is_empty() {
        return "no reflection entries".to_owned();
    }
    list.entries
        .iter()
        .map(render_row)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `show` rendering.
#[must_use]
pub fn render_text_show(row: &ReflectionRow) -> String {
    render_row(row)
}

/// One row's machine columns + the text body — no verdict language
/// (P9).
fn render_row(row: &ReflectionRow) -> String {
    use std::fmt::Write as _;
    let mut line = format!(
        "{} {} {} [{}..{})",
        row.entry_id,
        row.cadence,
        row.day,
        row.window_start.to_rfc3339(),
        row.window_end.to_rfc3339(),
    );
    let ratings = match (row.mood, row.energy) {
        (Some(mood), Some(energy)) => Some(format!("mood {mood}/5 energy {energy}/5")),
        (Some(mood), None) => Some(format!("mood {mood}/5")),
        (None, Some(energy)) => Some(format!("energy {energy}/5")),
        (None, None) => None,
    };
    if let Some(ratings) = ratings {
        let _ = write!(line, " {ratings}");
    }
    if !row.flags.is_empty() {
        let _ = write!(line, " flags:{}", row.flags.join(","));
    }
    if !row.extra_flags.is_empty() {
        let _ = write!(line, " +{}", row.extra_flags.join(","));
    }
    if !row.text.is_empty() {
        let _ = write!(line, " — {}", row.text);
    }
    line
}

/// The one-shot `journal` dispatch: the mode switch over
/// [`super::run_core`], each mode with its own handler and renderer.
pub async fn reflection_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<std::path::PathBuf>,
    invocation: ReflectionInvocation,
) -> u8 {
    match invocation {
        ReflectionInvocation::List { filters } => {
            super::run_core(
                io,
                config_path,
                render_text_list,
                move |config, store| async move { list(config, store, filters).await },
            )
            .await
        }
        ReflectionInvocation::Show(selector) => {
            super::run_core(
                io,
                config_path,
                render_text_show,
                move |config, store| async move { show(config, store, selector).await },
            )
            .await
        }
        ReflectionInvocation::Delete(selector) => {
            super::run_core(
                io,
                config_path,
                render_text_delete,
                move |config, store| async move { delete(config, store, selector).await },
            )
            .await
        }
        ReflectionInvocation::Edit { selector, mask } => {
            super::run_core(
                io,
                config_path,
                render_text_edit,
                move |config, store| async move { edit(config, store, selector, mask).await },
            )
            .await
        }
        ReflectionInvocation::Add { .. } => {
            super::run_core(
                io,
                config_path,
                render_text_add,
                |config, store| async move { add(config, store, invocation).await },
            )
            .await
        }
    }
}

/// The session-lane mode switch (`dispatch_line`): pure core reads and
/// mutations, no boot.
pub async fn reflection_line(
    app: &crate::App,
    calendar: DayCalendar,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: ReflectionInvocation,
) -> u8 {
    match invocation {
        ReflectionInvocation::List { filters } => {
            crate::commands::emit_result(io, Ok(list_on(app, &filters)), render_text_list)
        }
        ReflectionInvocation::Show(selector) => {
            crate::commands::emit_result(io, show_on(app, &selector), render_text_show)
        }
        ReflectionInvocation::Delete(selector) => {
            crate::commands::emit_result(io, delete_on(app, &selector).await, render_text_delete)
        }
        ReflectionInvocation::Edit { selector, mask } => {
            crate::commands::emit_result(io, edit_on(app, &selector, mask).await, render_text_edit)
        }
        ReflectionInvocation::Add {
            cadence,
            day,
            mood,
            energy,
            flags,
            text,
        } => crate::commands::emit_result(
            io,
            add_on(app, calendar, cadence, day, mood, energy, flags, text).await,
            render_text_add,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_is_total_over_the_clap_grammar() {
        for cmd in [
            crate::cli::ReflectionCmd::Add {
                cadence: None,
                day: None,
                mood: Some(3),
                energy: None,
                flag: vec![],
                text_words: vec!["slow".to_owned(), "morning".to_owned()],
            },
            crate::cli::ReflectionCmd::List {
                cadence: None,
                since: None,
                until: None,
            },
            crate::cli::ReflectionCmd::Show {
                selector: "abc".to_owned(),
            },
            crate::cli::ReflectionCmd::Edit {
                selector: "abc".to_owned(),
                text: None,
                clear_text: true,
                mood: None,
                clear_mood: true,
                energy: None,
                clear_energy: false,
                flag: vec![],
                unflag: vec![],
            },
            crate::cli::ReflectionCmd::Delete {
                selector: "abc".to_owned(),
            },
        ] {
            assert!(collapse(cmd).is_some(), "journal collapse is total");
        }
    }

    fn state_with(entries: &[taskboard_domain::ReflectionEntry]) -> AppState {
        AppState {
            reflection: taskboard_domain::ReflectionState {
                entries: entries.iter().map(|e| (e.id, e.clone())).collect(),
            },
            ..AppState::default()
        }
    }

    fn fixture(id: u128, deleted: bool) -> taskboard_domain::ReflectionEntry {
        taskboard_domain::ReflectionEntry {
            id: taskboard_domain::ids::ReflectionId::from_uuid(uuid::Uuid::from_u128(id)),
            cadence: ReviewCadence::Day,
            day: chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
            window_start: chrono::TimeZone::timestamp_opt(&chrono::Utc, 100, 0).unwrap(),
            window_end: chrono::TimeZone::timestamp_opt(&chrono::Utc, 100 + 24 * 3600, 0).unwrap(),
            text: String::new(),
            attributes: taskboard_domain::ReflectionAttributes::default(),
            created_at: chrono::TimeZone::timestamp_opt(&chrono::Utc, 100, 0).unwrap(),
            updated_at: chrono::TimeZone::timestamp_opt(&chrono::Utc, 100, 0).unwrap(),
            deleted,
        }
    }

    /// Resolution runs over live entries only: a tombstoned entry's
    /// selector is a typed unknown (Q-7), exact uuid and prefix still
    /// hit.
    #[test]
    fn resolution_skips_tombstones_and_hits_live_ids() {
        let live = fixture(0x0102_0304_0000_0001, false);
        let gone = fixture(0x0102_0304_0000_0002, true);
        let state = state_with(&[live.clone(), gone]);
        let id = resolve_reflection_entry(&state, &live.id.as_uuid().to_string())
            .expect("the live exact uuid hit");
        assert_eq!(id, live.id);
        let id = resolve_reflection_entry(&state, "010203")
            .expect_err("6 chars is a miss, never an ambiguity");
        assert!(matches!(id, CommandError::UnknownReflectionEntry { .. }));
        // The tombstone's own selector is a typed unknown.
        let gone_id = taskboard_domain::ids::ReflectionId::from_uuid(uuid::Uuid::from_u128(
            0x0000_0000_0000_0002,
        ));
        let err = resolve_reflection_entry(&state, &gone_id.as_uuid().to_string())
            .expect_err("a tombstone resolves to nothing");
        assert!(matches!(err, CommandError::UnknownReflectionEntry { .. }));
    }

    /// Minted v7 ids cannot be forced to collide hermetically —
    /// hand-built twins sharing an 8-char prefix (the
    /// `resolve_time_entry` test module's own `Uuid::from_u128`
    /// pattern) name both candidates typed.
    #[test]
    fn resolution_names_both_candidates_on_a_prefix_collision() {
        let twin_a = fixture(0x1234_0000_0000_0001, false);
        let twin_b = fixture(0x1234_0000_0000_0002, false);
        let state = state_with(&[twin_a.clone(), twin_b.clone()]);
        let err = resolve_reflection_entry(&state, "00000000")
            .expect_err("the shared prefix is ambiguous");
        let CommandError::AmbiguousReflectionEntry { candidates } = err else {
            panic!("the collision is typed ambiguous");
        };
        assert_eq!(candidates.len(), 2);
        assert!(
            candidates.contains(&twin_a.id.as_uuid().to_string())
                && candidates.contains(&twin_b.id.as_uuid().to_string())
        );
    }

    /// The change-mask double-Option semantics: set, clear, and
    /// unchanged collapse to distinct masks.
    #[test]
    fn collapse_encodes_the_edit_mask_pairs() {
        let ReflectionInvocation::Edit { mask, .. } = collapse(crate::cli::ReflectionCmd::Edit {
            selector: "s".to_owned(),
            text: Some("new".to_owned()),
            clear_text: false,
            mood: Some(2),
            clear_mood: false,
            energy: None,
            clear_energy: true,
            flag: vec![],
            unflag: vec![],
        })
        .expect("collapses") else {
            unreachable!("the argv selects edit")
        };
        assert_eq!(mask.text, Some("new".to_owned()));
        assert_eq!(mask.mood, Some(Some(2)), "set");
        assert_eq!(mask.energy, Some(None), "clear");
        assert!(mask.flags_add.is_empty() && mask.flags_remove.is_empty());
    }
}

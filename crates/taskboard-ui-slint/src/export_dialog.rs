// SPDX-License-Identifier: AGPL-3.0-only

//! The export dialog's presentation VM (the J2 export plan W-1–W-5):
//! the adapter-owned buffer behind `ui/compositions/export-dialog.slint`.
//! One truth lives here — the widget never holds state (the `TimeField`
//! contract): every edit mirrors up the moment it happens and the push
//! pass re-seeds the model from this buffer each refresh.
//!
//! The dialog is presentation state, not a [`Flow`](crate::view_state::Flow)
//! variant: it is entity-less (R-1), dispatches nothing until Run
//! (US-1/S1), and its escape carries no flow machinery to unwind — the
//! close seam drops the buffer and the pending reply receiver
//! (US-2/S2's escape law: the app-side run completes or refuses
//! independently).
//!
//! Destination semantics (D-2/D-3): the field pre-fills EMPTY with the
//! config default rendered as the placeholder; an empty field mints
//! `out: None` and the engine resolves the config default — the pdf
//! leg's `[export] pdf_dir` + the dated file name, the csv/json leg's
//! export dir + the dated file name. A non-empty field overrides. The
//! dated file name is deliberately NOT pre-filled as text: it is
//! engine-derived at run time (the dialog-open instant would freeze
//! yesterday's date into the pre-fill), and minting the engine's file
//! name format presentation-side would duplicate the engine (the O-5
//! trap) — the written-path fact in the outcome area shows the real
//! path after the run.

use crate::requests::{
    ExportDialogDefaults, ExportFamily, ExportFileFormat, ExportRefusal, ExportRunOutcome,
    PdfLayoutChoice,
};

/// The index contract the pickers speak (documented once, mapped at
/// the seams): family 0/1 = Tracking/Pdf; format 0/1 = Csv/Json;
/// layout 0..4 = Fold/A4/A5/A6 (the typed closed grammar — the OP2
/// trap: never free text). Field indices: 0/1 = since/until dates,
/// 2/3 = task/board selectors, 4 = destination.
pub(crate) const FAMILY_PICKED: i32 = 0;
pub(crate) const FORMAT_PICKED: i32 = 1;
pub(crate) const LAYOUT_PICKED: i32 = 2;
pub(crate) const FIELD_SINCE: i32 = 0;
pub(crate) const FIELD_UNTIL: i32 = 1;
pub(crate) const FIELD_TASK: i32 = 2;
pub(crate) const FIELD_BOARD: i32 = 3;
pub(crate) const FIELD_DESTINATION: i32 = 4;

/// The outcome area's kind indices (the fold grammar — flags and
/// indices, never caption text): 0 = nothing yet, 1 = the written
/// file fact, 2 = the typed refusal.
pub(crate) const OUTCOME_NONE: i32 = 0;
pub(crate) const OUTCOME_WRITTEN: i32 = 1;
pub(crate) const OUTCOME_REFUSED: i32 = 2;

/// The adapter-owned dialog buffer (the settings VM's sibling).
#[allow(clippy::struct_excessive_bools)]
// A dialog draft, not a state machine: the per-affordance busy flags
// (D-4's law — Run, Preview and Copy each hold their own) plus the
// pane's cap/settled facts mirror the dialog's independent controls;
// grouping them into an enum would invent couplings the draft does
// not have (the SettingsSnapshot precedent).
pub(crate) struct ExportVm {
    /// The picked data family (the tracking leg by default).
    pub(crate) family: ExportFamily,
    /// The tracking leg's payload format (`Csv` the default).
    pub(crate) format: ExportFileFormat,
    /// The pdf leg's print layout (`Fold` the default — the CLI's
    /// grammar).
    pub(crate) layout: PdfLayoutChoice,
    /// The since date field's verbatim text (empty = no filter).
    pub(crate) since: String,
    /// The until date field's verbatim text (empty = no filter).
    pub(crate) until: String,
    /// The task selector field's text (empty = no filter).
    pub(crate) task: String,
    /// The board selector field's text (empty = no filter).
    pub(crate) board: String,
    /// The destination field's text (empty = the engine's config
    /// default — the D-2/D-3 default, see the module doc).
    pub(crate) destination: String,
    /// The open-after-export checkbox (the pdf leg's control; seeded
    /// from `[export] open_after_export` — D-2).
    pub(crate) open_after: bool,
    /// The busy posture between the Run dispatch and the typed reply
    /// (D-5): the Run control disables and the dialog stays mounted.
    pub(crate) busy: bool,
    /// The settled typed outcome (US-2/S3, US-3/S3): `None` before
    /// the first run.
    pub(crate) outcome: Option<ExportRunOutcome>,
    // -- the preview/clipboard lane (the D2 preview plan W-1..W-4) --
    /// The per-affordance busy flags (D-4): the preview button, the
    /// copy button and the Run button each hold their own.
    pub(crate) preview_busy: bool,
    pub(crate) copy_busy: bool,
    /// The pane's settled facts (W-3): the engine payload verbatim
    /// (one generator, two sinks) plus the typed cap fact (D-2).
    pub(crate) preview_shown: bool,
    pub(crate) preview_text: String,
    pub(crate) preview_truncated: bool,
    /// The settled preview refusal kind (US-1/S4): `None` = none.
    pub(crate) preview_settled_refusal: Option<ExportRefusal>,
    /// The clipboard fold's settled facts (W-3's twin): the
    /// copied-byte fact or the refusal kind.
    pub(crate) copy_settled: bool,
    pub(crate) copy_bytes: i32,
    pub(crate) copy_refusal: Option<ExportRefusal>,
}

impl Default for ExportVm {
    fn default() -> Self {
        Self {
            family: ExportFamily::Tracking,
            format: ExportFileFormat::Csv,
            layout: PdfLayoutChoice::Fold,
            since: String::new(),
            until: String::new(),
            task: String::new(),
            board: String::new(),
            destination: String::new(),
            open_after: false,
            busy: false,
            outcome: None,
            preview_busy: false,
            copy_busy: false,
            preview_shown: false,
            preview_text: String::new(),
            preview_truncated: false,
            preview_settled_refusal: None,
            copy_settled: false,
            copy_bytes: 0,
            copy_refusal: None,
        }
    }
}

impl ExportVm {
    /// Seeds a fresh VM from the hand-over defaults (D-2/D-3): the
    /// config postures ride in, every filter starts empty, `Csv`
    /// family/format/layout defaults match the CLI's.
    pub(crate) fn seeded(defaults: &ExportDialogDefaults) -> Self {
        Self {
            family: ExportFamily::Tracking,
            format: ExportFileFormat::Csv,
            layout: PdfLayoutChoice::Fold,
            since: String::new(),
            until: String::new(),
            task: String::new(),
            board: String::new(),
            destination: String::new(),
            open_after: defaults.open_after,
            busy: false,
            outcome: None,
            preview_busy: false,
            copy_busy: false,
            preview_shown: false,
            preview_text: String::new(),
            preview_truncated: false,
            preview_settled_refusal: None,
            copy_settled: false,
            copy_bytes: 0,
            copy_refusal: None,
        }
    }

    /// The date field's validity fold (US-2/S1): empty is neutral (no
    /// filter), a parseable `YYYY-MM-DD` is valid, anything else is
    /// invalid — the invalid fact rides the field's validity flag,
    /// never caption text.
    pub(crate) fn date_validity(text: &str) -> crate::FieldValidity {
        if text.is_empty() {
            return crate::FieldValidity::Neutral;
        }
        if chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok() {
            crate::FieldValidity::Valid
        } else {
            crate::FieldValidity::Invalid
        }
    }

    /// Parses the date field into the spec's typed option (the mint
    /// halves of `date_validity` — the trust boundary never trusts
    /// the widget's rendered flag).
    pub(crate) fn parse_date(text: &str) -> Option<chrono::NaiveDate> {
        if text.is_empty() {
            None
        } else {
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
        }
    }

    /// Whether the whole draft mints a dispatchable spec (the Run
    /// gate): never while busy, never for the pdf family on a build
    /// without the feature (D-4's honest-disabled arm — the flag is
    /// the oracle, never the caption), and never with an invalid
    /// date field.
    pub(crate) fn run_enabled(&self, defaults: &ExportDialogDefaults) -> bool {
        if self.busy {
            return false;
        }
        if self.family == ExportFamily::Pdf && !defaults.pdf_available {
            return false;
        }
        Self::date_validity(&self.since) != crate::FieldValidity::Invalid
            && Self::date_validity(&self.until) != crate::FieldValidity::Invalid
    }

    /// Mints the spec the Run dispatch carries (W-4): the fields are
    /// the buffer's verbatim state (US-2/AC-1, US-3/AC-1/AC-2's seam
    /// oracle). `out` is `Some` only for a non-empty destination
    /// field — the empty field is the engine-default contract.
    pub(crate) fn mint_spec(&self) -> crate::requests::ExportSpec {
        crate::requests::ExportSpec {
            what: self.family,
            format: self.format,
            since: Self::parse_date(&self.since),
            until: Self::parse_date(&self.until),
            task: (!self.task.trim().is_empty()).then(|| self.task.trim().to_owned()),
            board: (!self.board.trim().is_empty()).then(|| self.board.trim().to_owned()),
            layout: (self.family == ExportFamily::Pdf).then_some(self.layout),
            out: (!self.destination.trim().is_empty())
                .then(|| std::path::PathBuf::from(self.destination.trim())),
            open_after: self.open_after,
        }
    }

    /// The refusal kind's dialog index (the rendering address; the
    /// canonical order lives in `requests` — one table, no drift).
    pub(crate) fn refusal_index(kind: ExportRefusal) -> i32 {
        crate::requests::refusal_index(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kills: any drift in the `Default` buffer's initial flags (the
    /// measurement's dark region — the journeys always seed from the
    /// hand-over defaults, so `Default` never executes there). The
    /// defaults are the dialog's pre-open posture: Tracking/Csv/Fold,
    /// every field empty, nothing busy, no settled outcome.
    #[test]
    fn default_vm_starts_at_the_dialogs_pre_open_posture() {
        let vm = ExportVm::default();
        assert_eq!(vm.family, ExportFamily::Tracking);
        assert_eq!(vm.format, ExportFileFormat::Csv);
        assert_eq!(vm.layout, PdfLayoutChoice::Fold);
        for field in [&vm.since, &vm.until, &vm.task, &vm.board, &vm.destination] {
            assert!(field.is_empty(), "every filter starts empty: {field:?}");
        }
        assert!(!vm.open_after);
        assert!(!vm.busy);
        assert!(!vm.preview_busy);
        assert!(!vm.copy_busy);
        assert!(!vm.preview_shown);
        assert!(!vm.preview_truncated);
        assert!(!vm.copy_settled);
        assert_eq!(vm.copy_bytes, 0);
        assert_eq!(vm.outcome, None);
        assert_eq!(vm.preview_text, "");
        assert_eq!(vm.preview_settled_refusal, None);
        assert_eq!(vm.copy_refusal, None);
    }

    /// Kills: a format-string drift or an empty/malformed handling
    /// swap in `parse_date` — the mint halves of the validity fold.
    #[test]
    fn parse_date_folds_empty_valid_and_malformed() {
        assert_eq!(ExportVm::parse_date(""), None, "empty is no filter");
        assert_eq!(
            ExportVm::parse_date("2026-11-25"),
            chrono::NaiveDate::from_ymd_opt(2026, 11, 25),
            "the YYYY-MM-DD form parses"
        );
        assert_eq!(ExportVm::parse_date("not a date"), None);
        assert_eq!(ExportVm::parse_date("2026-13-40"), None, "out-of-range");
        assert_eq!(
            ExportVm::parse_date("25/11/2026"),
            None,
            "a non-canonical separator is invalid, never reinterpreted"
        );
    }

    /// Kills: the `out`/`layout`/filter arms of `mint_spec` — the
    /// default VM's product is the engine-default contract (empty
    /// destination mints `None`, the tracking leg mints no layout, the
    /// empty selectors mint no filters); a non-empty destination
    /// mints `Some`, verbatim.
    #[test]
    fn mint_spec_default_vm_mints_the_engine_default_contract() {
        let vm = ExportVm::default();
        let spec = vm.mint_spec();
        assert_eq!(spec.what, ExportFamily::Tracking);
        assert_eq!(spec.format, ExportFileFormat::Csv);
        assert_eq!(spec.since, None);
        assert_eq!(spec.until, None);
        assert_eq!(spec.task, None);
        assert_eq!(spec.board, None);
        assert_eq!(spec.layout, None, "the tracking leg carries no layout");
        assert_eq!(
            spec.out, None,
            "the empty destination is the engine-default contract (D-2/D-3)"
        );
        assert!(!spec.open_after);

        // A non-empty destination mints Some, verbatim.
        let vm = ExportVm {
            destination: " /tmp/exports/november.csv ".to_owned(),
            ..ExportVm::default()
        };
        let spec = vm.mint_spec();
        assert_eq!(
            spec.out,
            Some(std::path::PathBuf::from("/tmp/exports/november.csv")),
            "the destination mints trimmed"
        );
    }

    /// Kills: any swapped or duplicate entry in the refusal index
    /// table (`requests::refusal_index` is the dialog's rendering
    /// address — the mapping is an address space, so every kind sits
    /// at its literal pinned row and the rows are pairwise distinct).
    /// The expectations are literals, not re-reads of the table: a
    /// swapped pair must fail here.
    #[test]
    fn refusal_index_maps_every_kind_to_a_distinct_canonical_row() {
        let rows: [(crate::requests::ExportRefusal, i32); 12] = [
            (crate::requests::ExportRefusal::PdfUnavailable, 0),
            (crate::requests::ExportRefusal::FiltersNotApplicable, 1),
            (crate::requests::ExportRefusal::OptionsNotApplicable, 2),
            (crate::requests::ExportRefusal::BadTaskFilter, 3),
            (crate::requests::ExportRefusal::BadBoardFilter, 4),
            (crate::requests::ExportRefusal::UnknownPdfLayout, 5),
            (crate::requests::ExportRefusal::PdfPath, 6),
            (crate::requests::ExportRefusal::PdfRender, 7),
            (crate::requests::ExportRefusal::DestinationUnwritable, 8),
            (crate::requests::ExportRefusal::LaneDropped, 9),
            (crate::requests::ExportRefusal::EngineRefused, 10),
            (crate::requests::ExportRefusal::ClipboardUnavailable, 11),
        ];
        let mut seen = std::collections::BTreeMap::new();
        for (kind, expected) in rows {
            let index = ExportVm::refusal_index(kind);
            assert_eq!(
                index, expected,
                "{kind:?} sits at its canonical row {expected}"
            );
            assert!(
                seen.insert(index, kind).is_none(),
                "indices are pairwise distinct: {index}"
            );
        }
    }
}

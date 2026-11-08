// SPDX-License-Identifier: AGPL-3.0-only

//! Per-level surface state (W5b D-H2): every adapter-side interaction
//! slot a flow owns lives in its level's [`LevelState`], minted at
//! [`crate::surfaces::Surfaces::push`] through the one total
//! constructor and dropped with the level — one lifetime rule replaces
//! every per-flow "wipe when the flow is not X" rule. Slint-free like
//! [`crate::due_edit`]: strings, domain ids and [`DuePickerState`]
//! only; the adapter maps state to window calls.
//!
//! The family is keyed by state *family*, not by tag (a tag enum would
//! be a tag-per-identical-payload): the pairing tag ↔ family is
//! enforced at [`LevelState::mint`], the only constructor. Deliberate
//! leniency: [`LevelState::Text`] is shared by ten tags — their
//! buffer *semantics* differ (trim vs. verbatim, seed vs. empty) but
//! those live in the flows' commit paths; the family only owns the
//! lifetime.
//!
//! [`FocusAnchor`] rides every level (D-H7): the mount focus anchor
//! the *parent* carries, restored on the child's exit. The `Grid` /
//! `Field` arms are the date editor's phase vocabulary
//! (`FocusHandover`'s); the flat flows' exits restore the view scope —
//! today's flowless hand-off — so their parents record [`FocusAnchor::View`].

use crate::due_edit::DuePickerState;
use crate::surfaces::SurfaceKind;

/// The mount focus anchor (D-H7): what a level's exit restores into
/// the level below it. The adapter maps the arm to the actual focus
/// call; `None` restores nothing beyond the existing flowless
/// hand-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FocusAnchor {
    #[default]
    None,
    /// The active view scope re-anchors (the flowless hand-off's
    /// `invoke_refocus_view` target) — every flat level's exit
    /// restore. The date editor's phase arms (`Grid`/`Field`) enter
    /// with the first nested parent (chunk 3's ports); the flat
    /// chain never mints them.
    View,
}

/// The date editor's adapter trio + companions (r2 D25, r4 D25/D26):
/// the typed buffer, its echo, the picker cursor/month/time, the time
/// combobox's visible text (the `--:--` value echo), the quick-list
/// filter latch, and the programmatic-focus-steal latch (armed by the
/// phase steps, consumed by the settle funnel — the level owns it).
/// A commit consumes all three buffers; the clean close keeps the
/// buffer/echo (D25) — unobservable past the close, the level dies
/// there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DateEditorState {
    pub(crate) buffer: String,
    pub(crate) echo: String,
    pub(crate) picker: Option<DuePickerState>,
    pub(crate) time_text: String,
    pub(crate) time_filter_armed: bool,
    pub(crate) focus_steal: bool,
    pub(crate) anchor: FocusAnchor,
}

/// The entries form's adapter-owned state (r4 D-TS4): the seed, the
/// grid (the picker's presence), the three typed texts, and the two
/// quick-list filter latches — wiped with the level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrackFormState {
    pub(crate) seed: Option<taskboard_domain::TimeEntryId>,
    pub(crate) picker: Option<DuePickerState>,
    pub(crate) date_text: String,
    pub(crate) start_text: String,
    pub(crate) end_text: String,
    pub(crate) start_filter_armed: bool,
    pub(crate) end_filter_armed: bool,
    pub(crate) anchor: FocusAnchor,
}

/// The placement form's two text mirrors (#622, D-3): the due buffers'
/// lifecycle — updated by the view's keystroke seams, read only at the
/// commit funnel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlanPlaceState {
    pub(crate) time_text: String,
    pub(crate) effort_text: String,
    pub(crate) anchor: FocusAnchor,
}

/// The review walk's adapter-owned buffers (DR-7 chunk 3): the prompt
/// answers (one slot per template prompt; survives stage-back and the
/// planner leg) and the armed date field's text. The date text retires
/// with the N3b port (chunk 3): the `WalkDatePick` child owns it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WalkState {
    pub(crate) answers: Vec<String>,
    pub(crate) date_text: String,
    pub(crate) anchor: FocusAnchor,
}

/// The standalone calendar picker's level family (chunk 3's N2 port):
/// the entries form's date grid as a child level — the r4 D-TS4
/// `DuePickerState` machinery verbatim (the time half unused; the
/// family mounts date-only). Family-default is the epoch placeholder:
/// the opening seam re-seeds through [`DuePickerState::open`] before
/// any render reads it (the `TrackFormState` seeding pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickerState {
    pub(crate) picker: crate::due_edit::DuePickerState,
    pub(crate) anchor: FocusAnchor,
}

/// The walk's Pick-a-date child's typed text (chunk 3's N3b port):
/// one field's mirror, dropped with the child level — the commit
/// parses it adapter-side and completes the child with the instant,
/// so the child's death is the clear (the one per-field wipe retires).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WalkDateState {
    pub(crate) date_text: String,
    pub(crate) anchor: FocusAnchor,
}

/// One level's interaction state (D2): minted family-default at push,
/// dropped with the level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LevelState {
    /// A view level: the anchor only (the board/subject facts stay
    /// root-level `ViewState` fields — chunk 1 D6).
    View {
        anchor: FocusAnchor,
    },
    /// The shared text buffer's tenants (the create/rename/capture
    /// rows, the title and description editors, the palette query,
    /// the concentration overlay's quick note): one buffer per
    /// level.
    Text {
        buffer: String,
        anchor: FocusAnchor,
    },
    DateEditor(DateEditorState),
    TrackForm(TrackFormState),
    PlanPlace(PlanPlaceState),
    Walk(WalkState),
    /// The standalone calendar-picker family (the entries form's N2
    /// child).
    Picker(PickerState),
    /// The walk's Pick-a-date child's typed field (N3b).
    WalkDate(WalkDateState),
    /// Every stateless flow (the none/move/overlay families — no
    /// text, no picker, no form): the anchor is all a level owns.
    Bare {
        anchor: FocusAnchor,
    },
}

impl LevelState {
    /// The one minting constructor (D1): a total match over
    /// [`SurfaceKind`], so a state family can never be paired with the
    /// wrong surface tag. Everything mints family-default.
    #[must_use]
    pub(crate) fn mint(kind: SurfaceKind) -> Self {
        match kind {
            SurfaceKind::View(_) => Self::View {
                anchor: FocusAnchor::None,
            },
            SurfaceKind::Flow(tag) => match tag {
                crate::surfaces::FlowTag::EditDate => Self::DateEditor(DateEditorState {
                    buffer: String::new(),
                    echo: String::new(),
                    picker: None,
                    time_text: String::from("--:--"),
                    time_filter_armed: false,
                    focus_steal: false,
                    anchor: FocusAnchor::None,
                }),
                crate::surfaces::FlowTag::TrackEntryForm => Self::TrackForm(TrackFormState {
                    seed: None,
                    picker: None,
                    date_text: String::new(),
                    start_text: String::new(),
                    end_text: String::new(),
                    start_filter_armed: false,
                    end_filter_armed: false,
                    anchor: FocusAnchor::None,
                }),
                crate::surfaces::FlowTag::PlanPlace => Self::PlanPlace(PlanPlaceState {
                    time_text: String::new(),
                    effort_text: String::new(),
                    anchor: FocusAnchor::None,
                }),
                crate::surfaces::FlowTag::ReviewWalk => Self::Walk(WalkState {
                    answers: Vec::new(),
                    date_text: String::new(),
                    anchor: FocusAnchor::None,
                }),
                // The epoch placeholder: the opening seam re-seeds
                // through `DuePickerState::open` before any render
                // reads the family (the struct doc).
                crate::surfaces::FlowTag::EntryDatePicker => {
                    Self::Picker(PickerState {
                        picker: crate::due_edit::DuePickerState::from_parts(
                            chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
                                .expect("the epoch placeholder"),
                            None,
                        ),
                        anchor: FocusAnchor::None,
                    })
                }
                crate::surfaces::FlowTag::WalkDatePick => {
                    Self::WalkDate(WalkDateState {
                        date_text: String::new(),
                        anchor: FocusAnchor::None,
                    })
                }
                crate::surfaces::FlowTag::None
                | crate::surfaces::FlowTag::Move
                | crate::surfaces::FlowTag::MoveStack
                | crate::surfaces::FlowTag::TrackEntries
                | crate::surfaces::FlowTag::DragMove
                | crate::surfaces::FlowTag::WalkFilePick => Self::Bare {
                    anchor: FocusAnchor::None,
                },
                crate::surfaces::FlowTag::Create
                | crate::surfaces::FlowTag::CreateStack
                | crate::surfaces::FlowTag::EditTitle
                | crate::surfaces::FlowTag::EditDescription
                | crate::surfaces::FlowTag::Capture
                | crate::surfaces::FlowTag::CreateBoard
                | crate::surfaces::FlowTag::RenameStack
                | crate::surfaces::FlowTag::RenameBoard
                | crate::surfaces::FlowTag::Palette
                // The concentration overlay's quick note rides the
                // shared buffer (the capture field's discipline).
                | crate::surfaces::FlowTag::FocusMode => Self::Text {
                    buffer: String::new(),
                    anchor: FocusAnchor::None,
                },
            },
        }
    }

    #[must_use]
    pub(crate) const fn anchor(&self) -> FocusAnchor {
        match self {
            Self::View { anchor }
            | Self::Text { anchor, .. }
            | Self::Bare { anchor }
            | Self::DateEditor(DateEditorState { anchor, .. })
            | Self::TrackForm(TrackFormState { anchor, .. })
            | Self::PlanPlace(PlanPlaceState { anchor, .. })
            | Self::Walk(WalkState { anchor, .. })
            | Self::Picker(PickerState { anchor, .. })
            | Self::WalkDate(WalkDateState { anchor, .. }) => *anchor,
        }
    }

    pub(crate) fn set_anchor(&mut self, anchor: FocusAnchor) {
        match self {
            Self::View { anchor: slot }
            | Self::Text { anchor: slot, .. }
            | Self::Bare { anchor: slot } => *slot = anchor,
            Self::DateEditor(state) => state.anchor = anchor,
            Self::TrackForm(state) => state.anchor = anchor,
            Self::PlanPlace(state) => state.anchor = anchor,
            Self::Walk(state) => state.anchor = anchor,
            Self::Picker(state) => state.anchor = anchor,
            Self::WalkDate(state) => state.anchor = anchor,
        }
    }

    /// The innermost `Text` level's buffer write seam (the adapter's
    /// keystroke mirror; D9 rule 1). `None` when no text level is
    /// live — today's post-close writes are already clears, and the
    /// accessor's no-op is byte-equal to a cleared-buffer write.
    pub(crate) fn text_buffer_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Text { buffer, .. } => Some(buffer),
            _ => None,
        }
    }

    pub(crate) fn as_date_editor_mut(&mut self) -> Option<&mut DateEditorState> {
        match self {
            Self::DateEditor(state) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn as_date_editor(&self) -> Option<&DateEditorState> {
        match self {
            Self::DateEditor(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn as_track_form_mut(&mut self) -> Option<&mut TrackFormState> {
        match self {
            Self::TrackForm(state) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn as_track_form(&self) -> Option<&TrackFormState> {
        match self {
            Self::TrackForm(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn as_plan_place_mut(&mut self) -> Option<&mut PlanPlaceState> {
        match self {
            Self::PlanPlace(state) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn as_plan_place(&self) -> Option<&PlanPlaceState> {
        match self {
            Self::PlanPlace(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn as_walk_mut(&mut self) -> Option<&mut WalkState> {
        match self {
            Self::Walk(state) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn as_walk(&self) -> Option<&WalkState> {
        match self {
            Self::Walk(state) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn text_buffer(&self) -> Option<&str> {
        match self {
            Self::Text { buffer, .. } => Some(buffer),
            _ => None,
        }
    }

    /// Whether the state is exactly what [`Self::mint`] would produce
    /// for the level's kind — the proptest's "written state is
    /// observable" oracle (and the clean-reopen rule's assertion
    /// shape).
    #[cfg(test)]
    pub(crate) fn is_default_for(&self, kind: SurfaceKind) -> bool {
        *self == Self::mint(kind)
    }

    /// Writes one family-specific fact through a test seam (the
    /// proptest's write arm): the state must then be observable
    /// through the family accessors until the level is dropped.
    #[cfg(test)]
    pub(crate) fn write_test_fact(&mut self) {
        match self {
            Self::Text { buffer, .. } => *buffer = "written".to_owned(),
            Self::DateEditor(state) => {
                state.buffer = "written".to_owned();
                state.picker = Some(DuePickerState::from_parts(
                    chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a valid date"),
                    None,
                ));
            }
            Self::TrackForm(state) => state.date_text = "written".to_owned(),
            Self::PlanPlace(state) => state.time_text = "written".to_owned(),
            Self::Walk(state) => state.date_text = "written".to_owned(),
            Self::Picker(state) => {
                state.picker = crate::due_edit::DuePickerState::from_parts(
                    chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a valid date"),
                    None,
                );
            }
            Self::WalkDate(state) => state.date_text = "written".to_owned(),
            // The anchor is the stateless families' one observable
            // fact.
            Self::View { .. } | Self::Bare { .. } => self.set_anchor(FocusAnchor::View),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::FlowTag;

    /// One state per family, minted through the constructor.
    fn state_of(tag: FlowTag) -> LevelState {
        LevelState::mint(SurfaceKind::Flow(tag))
    }

    #[test]
    fn minting_is_family_default_and_pairing_is_total() {
        // Every tag mints; the text tenants share the Text family, the
        // stateless tags share Bare.
        for tag in [
            FlowTag::Create,
            FlowTag::CreateStack,
            FlowTag::EditTitle,
            FlowTag::EditDescription,
            FlowTag::Capture,
            FlowTag::CreateBoard,
            FlowTag::RenameStack,
            FlowTag::RenameBoard,
            FlowTag::Palette,
        ] {
            assert!(matches!(state_of(tag), LevelState::Text { .. }));
        }
        for tag in [
            FlowTag::None,
            FlowTag::Move,
            FlowTag::MoveStack,
            FlowTag::TrackEntries,
            FlowTag::DragMove,
            FlowTag::WalkFilePick,
        ] {
            assert!(matches!(state_of(tag), LevelState::Bare { .. }));
        }
        assert!(matches!(
            state_of(FlowTag::EditDate),
            LevelState::DateEditor(_)
        ));
        assert!(matches!(
            state_of(FlowTag::TrackEntryForm),
            LevelState::TrackForm(_)
        ));
        assert!(matches!(
            state_of(FlowTag::PlanPlace),
            LevelState::PlanPlace(_)
        ));
        assert!(matches!(state_of(FlowTag::ReviewWalk), LevelState::Walk(_)));
        assert!(matches!(
            state_of(FlowTag::EntryDatePicker),
            LevelState::Picker(_)
        ));
        assert!(matches!(
            state_of(FlowTag::WalkDatePick),
            LevelState::WalkDate(_)
        ));
        assert!(matches!(
            LevelState::mint(SurfaceKind::View(crate::view_state::ViewId::Board)),
            LevelState::View { .. }
        ));
    }

    #[test]
    fn the_date_editor_mints_the_value_echo_not_the_empty_string() {
        let LevelState::DateEditor(state) = state_of(FlowTag::EditDate) else {
            panic!("the date editor's family");
        };
        assert_eq!(state.time_text, "--:--");
        assert!(state.buffer.is_empty() && state.echo.is_empty());
        assert!(state.picker.is_none());
    }

    /// The per-family clean-reopen rule (D6.2, at the mint): a level
    /// written through the test seam, dropped, and re-minted is
    /// family-default — the drop is the wipe.
    #[test]
    fn a_written_state_never_survives_its_level() {
        for tag in [
            FlowTag::Create,
            FlowTag::Palette,
            FlowTag::FocusMode,
            FlowTag::EditDate,
            FlowTag::TrackEntryForm,
            FlowTag::PlanPlace,
            FlowTag::ReviewWalk,
            FlowTag::EntryDatePicker,
            FlowTag::WalkDatePick,
            FlowTag::WalkFilePick,
        ] {
            let mut state = state_of(tag);
            state.write_test_fact();
            assert!(!state.is_default_for(SurfaceKind::Flow(tag)));
            drop(state);
            assert!(
                state_of(tag).is_default_for(SurfaceKind::Flow(tag)),
                "the re-minted level starts clean"
            );
        }
    }

    #[test]
    fn the_anchor_reads_and_writes_through_every_family() {
        for kind in [
            SurfaceKind::View(crate::view_state::ViewId::Board),
            SurfaceKind::Flow(FlowTag::Create),
            SurfaceKind::Flow(FlowTag::EditDate),
            SurfaceKind::Flow(FlowTag::TrackEntryForm),
            SurfaceKind::Flow(FlowTag::PlanPlace),
            SurfaceKind::Flow(FlowTag::ReviewWalk),
            SurfaceKind::Flow(FlowTag::Capture),
        ] {
            let mut state = LevelState::mint(kind);
            assert_eq!(state.anchor(), FocusAnchor::None);
            state.set_anchor(FocusAnchor::View);
            assert_eq!(state.anchor(), FocusAnchor::View);
        }
    }
}

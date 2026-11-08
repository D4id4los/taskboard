// SPDX-License-Identifier: AGPL-3.0-only

//! The keybind context stack (round-5 chunk 7, D-KS1): the
//! declarative replacement for `KeymapShell::chain`'s boolean
//! reconstruction. A scope is a registration row evaluated (pulled)
//! at intercept time from already-mirrored state — no stored
//! registrations, so none can outlive its surface (the P5-class
//! stale-scope trap is avoided by construction: every arming
//! condition is a fact the adapter re-reads per keystroke).
//!
//! Ordering (round plan §4.7): focused-element scopes (innermost
//! first) → enclosing surface scopes (innermost first) → view → mode
//! → global. [`scope_stack`] is pure — no slint types; the adapter
//! passes the pulled facts in [`ScopeInputs`].

use std::collections::BTreeMap;

use crate::actions::ContextId;
use crate::shell::KeymapShell;
use crate::view_state::Flow;

/// The facts one intercept (or refresh pass) pulls for
/// [`scope_stack`]. Every field is already-mirrored state — nothing
/// here is stored between keystrokes (D-KS1's self-healing rule).
#[derive(Debug, Clone, Copy)]
pub struct ScopeInputs<'a> {
    /// The shell's mode/menu/modal mirrors (the legacy chain's
    /// inputs, verbatim).
    pub shell: &'a KeymapShell,
    /// The due/scheduled picker panel is mounted (`due_picker.is_some()`
    /// in the adapter). Together with the `Flow::EditDate` flow and a
    /// non-focused grid this arms the F11 field-phase head (D-δ1).
    pub picker_open: bool,
    /// `EditingFacts.editor-focus-count > 0`: the multi-line editor's
    /// element scope rides the head (D-δ2).
    pub editor_focus: bool,
    /// The view-state flow, pulled per intercept (the `EditDate`
    /// arming's other half).
    pub flow: &'a Flow,
    /// The chain's flow tags, innermost first (D-H6/D9): the
    /// concatenation walk's input. Callers that don't describe a
    /// chain (the legacy synthetic states) pass empty — the legacy
    /// boolean arms stay as the fallback.
    pub chain: &'a [crate::surfaces::FlowTag],
    /// The pending stroke at the key doorway (`None` everywhere else —
    /// refresh-pass hint chains never arm the deltas).
    pub stroke: Option<crate::keymap::Stroke>,
    /// Whether the caller is the key doorway (`intercept`). The two
    /// deltas arm ONLY there: resolution happens at the doorway, so
    /// that is the only place a new context can change outcomes; the
    /// refresh-pass hint chains stay byte-legacy.
    pub at_key_doorway: bool,
}

impl ScopeInputs<'_> {
    /// D-δ1's arming predicate: the picker's FIELD phase — the
    /// `EditDate` flow holds, the picker panel is mounted, and the
    /// grid does NOT hold slint focus (the grid phase already rides
    /// the head through `date_picker_grid`).
    #[must_use]
    pub fn picker_field_phase(&self) -> bool {
        matches!(self.flow, Flow::EditDate { .. })
            && self.picker_open
            && !self.shell.date_picker_grid
    }

    /// D-δ1's ladder exemption: bare Return and bare Escape stay with
    /// the field. Both are pinned journeys
    /// (`the_due_picker_journeys_reach_the_boundary`: the field's
    /// Return commits the typed date; the field phase's Escape is the
    /// phase step back) — the field-phase head serves only the
    /// chord-class and month-navigation strokes, never these two.
    #[must_use]
    pub fn field_phase_head_exempt(&self) -> bool {
        let Some(stroke) = self.stroke else {
            return false;
        };
        is_bare(stroke) && matches!(stroke.named_name(), Some("Return" | "Escape"))
    }

    /// Whether the field-phase picker head arms under these facts.
    #[must_use]
    pub fn field_phase_head_arms(&self) -> bool {
        self.at_key_doorway && self.picker_field_phase() && !self.field_phase_head_exempt()
    }
}

/// The ordered context stack under the pulled facts. Byte-equal to
/// the retired `KeymapShell::chain` for every legacy state (the T-S1
/// port checklist pins every shape); the two declared deltas are the
/// field-phase picker head (D-δ1) and the editor head (D-δ2).
#[must_use]
pub fn scope_stack(inputs: &ScopeInputs) -> Vec<ContextId> {
    let shell = inputs.shell;
    if shell.menu.open {
        return vec![ContextId::Menu];
    }
    // The chain concatenation (D-H6/D9): the levels' declared
    // contexts innermost-first, stopping at the first masking head.
    // The nested children declare empty heads (`FlowTag::scope_head`'s
    // `_` arm), so for every reachable state the walk finds the mask
    // at the same position the flat single-level read did —
    // byte-equal. The boolean arms below stay as the fallback for the
    // legacy synthetic states the chain facts don't describe (the
    // T-S1 checklist).
    for tag in inputs.chain {
        let head = tag.scope_head();
        if !head.is_empty() {
            return head.to_vec();
        }
    }
    // The five masking modals' heads are each kind's declaration
    // (D-H6/D10 — `FlowTag::scope_head`); the arming facts stay the
    // `KeymapShell` booleans. Byte-equal to the retired hard-coded
    // vectors for every legacy state (the T-S1 checklist).
    if shell.palette {
        // The palette is the second modal context (D8).
        return crate::surfaces::FlowTag::Palette.scope_head().to_vec();
    }
    if shell.focus_overlay {
        // The concentration overlay is the third modal context (#496).
        return crate::surfaces::FlowTag::FocusMode.scope_head().to_vec();
    }
    if shell.track_entries {
        // The time-entries overlay is the fourth modal context
        // (r4 chunk 10, D-TS2).
        return crate::surfaces::FlowTag::TrackEntries.scope_head().to_vec();
    }
    if shell.review_walk {
        // The review walk is the fifth modal context (DR-7 chunk 3,
        // #638).
        return crate::surfaces::FlowTag::ReviewWalk.scope_head().to_vec();
    }
    let mut chain = Vec::with_capacity(5);
    // D-δ2: the editor element scope rides the head (non-masking,
    // keymap-unbound — its verbs are the command tables, not rows).
    // Doorway-only: the refresh-pass hint chains stay byte-legacy.
    if inputs.editor_focus && inputs.at_key_doorway {
        chain.push(ContextId::Editor);
    }
    // The picker surface's scope: the grid phase (round-3 D25) and —
    // the F11 delta — the field phase (D-δ1), both non-masking. The
    // field-phase arm carries the Return/Escape exemption.
    if shell.date_picker_grid || inputs.field_phase_head_arms() {
        chain.push(ContextId::DatePicker);
    }
    if shell.mode == crate::router::Mode::Insert {
        chain.push(ContextId::ViInsert);
        chain.push(ContextId::Global);
        return chain;
    }
    if let Some(view) = shell.view {
        chain.push(view);
    }
    chain.push(ContextId::ViNormal);
    chain.push(ContextId::Global);
    chain
}

/// The printable monopoly (D-KS2): while a text field holds slint
/// focus, it claims every monopolizable stroke outright — `intercept`
/// rejects (returns `Unhandled`) before any scope or resolver step,
/// so the focused field receives the key. Gated on the insert mode:
/// in every flowless state the D20 pull makes the mode Insert exactly
/// while `text_focus > 0`, so the gate reproduces today's insert-mask
/// protection verbatim (an assertion in T-M1) while keeping the vi
/// preset's Normal-mode editor ladder reachable. A pending keymap
/// prefix owns the next stroke (`C-x u` while typing is the undo, an
/// unbound follower aborts and is consumed) — the field never sees a
/// prefix's follower, exactly as under the retired insert mask.
#[must_use]
pub fn monopoly_declines(
    mode: crate::router::Mode,
    text_focus: usize,
    prefix_pending: bool,
    stroke: crate::keymap::Stroke,
) -> bool {
    mode == crate::router::Mode::Insert
        && text_focus > 0
        && !prefix_pending
        && !stroke.ctrl
        && !stroke.alt
        && !stroke.meta
        && stroke.is_monopolizable()
}

// The retired special cases (D-KS4), as declarative arms evaluated
// at the key doorway. Each arming fact is exactly the condition the
// imperative special case tested; each fires exactly where its
// original block fired (before the resolver, after the monopoly).
// The pin journeys are the acceptance contract — none of their
// assertions may move.

/// Which dropdown ladder global reports open (r4 D39/D40, round-5
/// D6 — the widgets' facts globals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LadderGlobal {
    /// `SettingsBoardFacts.open` — a remote row's local-board picker flyout.
    SettingsBoard,
    /// `BoardBarFacts.open` — the board-bar segment flyout.
    BoardBar,
    /// `TimeFieldFacts.open` — the quick-list flyout.
    TimeField,
}

/// The dropdown ladders' decline arm (special case D): while a
/// flyout is open, its keys decline the single key path — the
/// focused widget's modal arm owns them. Reproduces the retired
/// closure's stroke sets verbatim: the two selector ladders decline
/// the arrows (any modifiers), emacs' C-n/C-p, and Escape/Return;
/// the quick-list ladder declines the arrows, Escape and Return
/// (no chords).
#[must_use]
pub fn dropdown_ladder_declines(open: Option<LadderGlobal>, stroke: crate::keymap::Stroke) -> bool {
    let arrow = matches!(stroke.named_name(), Some("Up" | "Down"));
    let escape_or_return = matches!(stroke.named_name(), Some("Escape" | "Return"));
    match open {
        Some(LadderGlobal::SettingsBoard | LadderGlobal::BoardBar) => {
            arrow || escape_or_return || (stroke.ctrl && (stroke.key == 'n' || stroke.key == 'p'))
        }
        Some(LadderGlobal::TimeField) => arrow || escape_or_return,
        None => false,
    }
}

/// The timer setter's dismissal arm (special case A, r4 D33): bare
/// Escape ahead of the resolver — the panel is presentation state,
/// not a flow, so the dismissal is an explicit decline routed to the
/// existing `clear_timer_setter` funnel (nothing commits; the draft
/// clears with the panel).
#[must_use]
pub fn timer_setter_dismisses(open: bool, stroke: crate::keymap::Stroke) -> bool {
    open && is_bare(stroke) && stroke.named_name() == Some("Escape")
}

/// The due editor's Down arm (special case B, r2 D25 / round-3 D10):
/// while the `EditDate` flow holds, bare Down opens the picker
/// grid-focused — or, with the picker already open, performs the
/// field→grid focus hand-over. The stateful mechanics stay in the
/// adapter (`open_due_picker` / `FocusHandover::Grid`); the arm here
/// is only the declaration of which stroke under which fact fires it.
#[must_use]
pub fn due_down_arms(edit_date_flow: bool, stroke: crate::keymap::Stroke) -> bool {
    edit_date_flow && is_bare(stroke) && stroke.named_name() == Some("Down")
}

/// No modifier at all (the retired arms compared against the bare
/// parsed stroke).
const fn is_bare(stroke: crate::keymap::Stroke) -> bool {
    !stroke.ctrl && !stroke.alt && !stroke.shift && !stroke.meta
}

/// The activation reservation (special case C, r4 §2 D18): while an
/// activation stop holds focus, bare Return/Space belong to the
/// focused cell — the resolver is still fed first (prefix semantics
/// preserved), but an `Action` or `Unhandled` answer yields to the
/// cell. Disjoint from the monopoly by the facts (an activation stop
/// is not a text field — T-M4).
#[must_use]
pub fn activation_reserved(
    activation_focus: bool,
    menu_open: bool,
    palette: bool,
    stroke: crate::keymap::Stroke,
) -> bool {
    activation_focus
        && !menu_open
        && !palette
        && (stroke.key == '\n' || stroke.key == ' ')
        && is_bare(stroke)
}

/// One editor-scope command (D-KS3): what the adapter performs on the
/// focused editor target through the seam. The command model is
/// motion/surgery-neutral — the adapter computes the new text and
/// cursor byte offsets, the widget applies them to its `TextInput`
/// (the L1 boundary chunk 2 set: Rust never touches `TextInput`
/// internals; the widget's `invoke-editor-command` handler does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommand {
    /// C-b: one character back.
    CursorBackward,
    /// C-f: one character forward.
    CursorForward,
    /// C-p: one logical line up (documented leniency: logical lines,
    /// not visual — `TextInput` cursor APIs give offsets only).
    LineUp,
    /// C-n: one logical line down.
    LineDown,
    /// C-a: line start.
    LineStart,
    /// C-e: line end.
    LineEnd,
    /// C-k: kill to the line end (into the clipboard slot).
    KillToLineEnd,
    /// C-d: delete one character forward (or the selection).
    DeleteForward,
    /// C-Space: set the mark at the cursor.
    SetMark,
    /// C-w: cut the marked region into the clipboard slot.
    CutRegion,
    /// M-w: copy the marked region into the clipboard slot.
    CopyRegion,
    /// C-y: paste the clipboard slot at the cursor.
    Paste,
    /// Shift+Return (≡ the checkmark) and C-c C-c: commit through
    /// the existing `accepted` funnel.
    Commit,
    /// C-c C-k: the flow's discard — the same outcome as Escape
    /// today (the existing settle, never a new one). Escape itself
    /// is NOT rebound (the plan's explicit ruling).
    Discard,
    /// vi's `i`: back to insert — the shell's mode flip.
    EnterInsert,
}

impl EditorCommand {
    /// The widget-seam command code (see `ui/editor-scope.slint`'s
    /// table — kept in step by this mapping and the seam's comment).
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::CursorBackward => 0,
            Self::CursorForward => 1,
            Self::LineUp => 2,
            Self::LineDown => 3,
            Self::LineStart => 4,
            Self::LineEnd => 5,
            Self::KillToLineEnd => 6,
            Self::DeleteForward => 7,
            Self::SetMark => 8,
            Self::CutRegion => 9,
            Self::CopyRegion => 10,
            Self::Paste => 11,
            Self::Commit => 12,
            Self::Discard => 13,
            Self::EnterInsert => 14,
        }
    }
}

/// The editor command table (D-KS3): spelled strokes → commands per
/// preset. A `BTreeMap` for deterministic lookup order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditorTable(BTreeMap<Vec<crate::keymap::Stroke>, EditorCommand>);

impl EditorTable {
    /// The exact binding, if any.
    #[must_use]
    pub fn exact(&self, seq: &[crate::keymap::Stroke]) -> Option<EditorCommand> {
        self.0.get(seq).copied()
    }

    /// Whether any binding starts with `seq` (the prefix test).
    #[must_use]
    pub fn has_prefix(&self, seq: &[crate::keymap::Stroke]) -> bool {
        self.0.keys().any(|binding| binding.starts_with(seq))
    }

    fn bind(&mut self, spellings: &[&str], command: EditorCommand) {
        let seq = crate::keymap::stroke::parse_sequence(spellings.iter().copied())
            .expect("editor table spellings parse (const data)");
        self.0.insert(seq, command);
    }
}

/// The preset's editor command table (D-KS3). The emacs set is the
/// dispatch §8 list verbatim; the vi set is the deliberate strip
/// (h, l, j, k, 0, $, x, i — recorded; no yank/register grammar).
/// The `AltGr` limitation is recorded at the emacs M-w row (stage-1
/// plan §8): `AltGr` surfaces as Ctrl+Alt, which the table does not
/// bind — M-w is unreachable while `AltGr` is held, a layout-level
/// leniency, never a misfire.
#[must_use]
pub fn editor_table(preset: crate::keymap::Preset) -> EditorTable {
    let mut table = EditorTable::default();
    match preset {
        crate::keymap::Preset::Standard => {}
        crate::keymap::Preset::Emacs => {
            table.bind(&["Ctrl+b"], EditorCommand::CursorBackward);
            table.bind(&["Ctrl+f"], EditorCommand::CursorForward);
            table.bind(&["Ctrl+p"], EditorCommand::LineUp);
            table.bind(&["Ctrl+n"], EditorCommand::LineDown);
            table.bind(&["Ctrl+a"], EditorCommand::LineStart);
            table.bind(&["Ctrl+e"], EditorCommand::LineEnd);
            table.bind(&["Ctrl+k"], EditorCommand::KillToLineEnd);
            table.bind(&["Ctrl+d"], EditorCommand::DeleteForward);
            table.bind(&["Ctrl+Space"], EditorCommand::SetMark);
            table.bind(&["Ctrl+w"], EditorCommand::CutRegion);
            table.bind(&["Alt+w"], EditorCommand::CopyRegion);
            table.bind(&["Ctrl+y"], EditorCommand::Paste);
            table.bind(&["Ctrl+c", "Ctrl+c"], EditorCommand::Commit);
            table.bind(&["Ctrl+c", "Ctrl+k"], EditorCommand::Discard);
        }
        crate::keymap::Preset::Vi => {
            table.bind(&["h"], EditorCommand::CursorBackward);
            table.bind(&["l"], EditorCommand::CursorForward);
            table.bind(&["j"], EditorCommand::LineDown);
            table.bind(&["k"], EditorCommand::LineUp);
            table.bind(&["0"], EditorCommand::LineStart);
            table.bind(&["$"], EditorCommand::LineEnd);
            table.bind(&["x"], EditorCommand::DeleteForward);
            table.bind(&["i"], EditorCommand::EnterInsert);
        }
    }
    table
}

/// The editor state one command reads (D-KS3): the buffer, the
/// cursor and mark as UTF-8 byte offsets, and the single-slot
/// clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorState<'a> {
    pub buffer: &'a str,
    pub cursor: usize,
    pub mark: Option<usize>,
    pub clipboard: Option<&'a str>,
}

/// One command's outcome (D-KS3): the replacement text (`None` = the
/// buffer is untouched), the new cursor, the new mark, and the new
/// clipboard slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorEdit {
    pub text: Option<String>,
    pub cursor: usize,
    pub mark: Option<usize>,
    pub clipboard: Option<String>,
}

/// The editor command arithmetic (D-KS3), pure: the adapter feeds the
/// widget-mirrored state and writes the outcome back through the
/// seam. Documented leniencies (backlog-recorded): line motions move
/// by LOGICAL lines (`TextInput` cursor APIs give offsets only —
/// visual-line motion is backlog); C-k at a line end kills nothing
/// (no newline join); C-d deletes one character, never the region.
/// Motions keep the mark (the emacs region semantics: a motion after
/// C-Space extends the region); the region-consuming commands clear
/// it. Commit/Discard/EnterInsert are no-ops here (they ride the
/// adapter's funnels).
#[must_use]
pub fn apply_editor_command(state: &EditorState<'_>, command: EditorCommand) -> EditorEdit {
    let buffer = state.buffer;
    let cursor = floor_boundary(buffer, state.cursor);
    let mut edit = EditorEdit {
        text: None,
        cursor,
        mark: state.mark.map(|mark| floor_boundary(buffer, mark)),
        clipboard: state.clipboard.map(str::to_owned),
    };
    match command {
        EditorCommand::CursorBackward => edit.cursor = boundary_back(buffer, cursor),
        EditorCommand::CursorForward => edit.cursor = boundary_forward(buffer, cursor),
        EditorCommand::LineStart => edit.cursor = line_start(buffer, cursor),
        EditorCommand::LineEnd => edit.cursor = line_end(buffer, cursor),
        EditorCommand::LineUp => {
            let start = line_start(buffer, cursor);
            if start > 0 {
                let prev_end = start - 1;
                let prev_start = line_start(buffer, prev_end);
                edit.cursor = floor_boundary(buffer, (prev_start + cursor - start).min(prev_end));
            }
        }
        EditorCommand::LineDown => {
            let end = line_end(buffer, cursor);
            if end < buffer.len() {
                let next_start = end + 1;
                let next_end = line_end(buffer, next_start);
                let column = cursor - line_start(buffer, cursor);
                edit.cursor = floor_boundary(buffer, (next_start + column).min(next_end));
            }
        }
        EditorCommand::KillToLineEnd => {
            let end = line_end(buffer, cursor);
            if end > cursor {
                edit.clipboard = Some(buffer[cursor..end].to_owned());
                edit.text = Some(splice(buffer, cursor, end, ""));
            }
            edit.mark = None;
        }
        EditorCommand::DeleteForward => {
            if cursor < buffer.len() {
                edit.text = Some(splice(buffer, cursor, boundary_forward(buffer, cursor), ""));
            }
            edit.mark = None;
        }
        EditorCommand::SetMark => edit.mark = Some(cursor),
        EditorCommand::CutRegion | EditorCommand::CopyRegion => {
            if let Some(mark) = edit.mark {
                let (from, to) = (mark.min(cursor), mark.max(cursor));
                if to > from {
                    edit.clipboard = Some(buffer[from..to].to_owned());
                }
                if command == EditorCommand::CutRegion {
                    edit.text = Some(splice(buffer, from, to, ""));
                    edit.cursor = from;
                }
                edit.mark = None;
            }
        }
        EditorCommand::Paste => {
            if let Some(clip) = state.clipboard {
                edit.text = Some(splice(buffer, cursor, cursor, clip));
                edit.cursor = cursor + clip.len();
                edit.mark = None;
            }
        }
        EditorCommand::Commit | EditorCommand::Discard | EditorCommand::EnterInsert => {}
    }
    edit
}

/// The largest scalar boundary ≤ `at` (clamped to the buffer).
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// One scalar back (multi-byte characters move whole).
fn boundary_back(text: &str, at: usize) -> usize {
    if at == 0 {
        return 0;
    }
    floor_boundary(text, at - 1)
}

/// One scalar forward (multi-byte characters move whole).
fn boundary_forward(text: &str, at: usize) -> usize {
    let mut at = at;
    if at < text.len() {
        at += 1;
        while at < text.len() && !text.is_char_boundary(at) {
            at += 1;
        }
    }
    at
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |pos| pos + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |pos| at + pos)
}

fn splice(text: &str, from: usize, to: usize, with: &str) -> String {
    let mut out = String::with_capacity(text.len() - (to - from) + with.len());
    out.push_str(&text[..from]);
    out.push_str(with);
    out.push_str(&text[to..]);
    out
}

/// What the editor table did with one stroke (the resolver's
/// Resolution shape, scope-local).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorClaim {
    /// The table claims the stroke: `intercept` performs the command
    /// and consumes the event.
    Command(EditorCommand),
    /// The stroke entered (or stays in) a pending prefix — consumed,
    /// no command yet.
    Consumed,
    /// A fresh stroke the table doesn't shape at all: the ordinary
    /// resolver feed must see it.
    Pass,
}

/// The editor scope's LOCAL prefix accumulator (D-KS3's recorded
/// decision: the keymap resolver is not reused for editor commands).
/// Same prefix discipline as `KeymapResolver`, one scope-local
/// pending sequence.
#[derive(Debug, Default)]
pub struct EditorResolver {
    pending: Vec<crate::keymap::Stroke>,
}

impl EditorResolver {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// Feeds one stroke against the table. A completed prefix's
    /// stray second stroke aborts the accumulation and is consumed
    /// (the resolver's `Aborted` shape) — it does NOT fall through.
    pub fn feed(&mut self, stroke: crate::keymap::Stroke, table: &EditorTable) -> EditorClaim {
        let had_pending = !self.pending.is_empty();
        self.pending.push(stroke);

        if let Some(command) = table.exact(&self.pending) {
            self.pending.clear();
            return EditorClaim::Command(command);
        }
        if table.has_prefix(&self.pending) {
            return EditorClaim::Consumed;
        }
        self.pending.clear();
        if had_pending {
            EditorClaim::Consumed
        } else {
            EditorClaim::Pass
        }
    }

    /// Drops any pending accumulation (focus/flow ends).
    pub fn reset(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ContextId;
    use crate::router::Mode;
    use crate::view_state::{DateField, Flow};
    use proptest::prelude::*;
    use taskboard_domain::TaskId;

    fn inputs<'a>(
        shell: &'a KeymapShell,
        flow: &'a Flow,
        picker_open: bool,
        editor_focus: bool,
    ) -> ScopeInputs<'a> {
        ScopeInputs {
            shell,
            picker_open,
            editor_focus,
            flow,
            chain: &[],
            stroke: None,
            at_key_doorway: false,
        }
    }

    fn doorway<'a>(
        shell: &'a KeymapShell,
        flow: &'a Flow,
        picker_open: bool,
        editor_focus: bool,
        stroke: Option<crate::keymap::Stroke>,
    ) -> ScopeInputs<'a> {
        ScopeInputs {
            at_key_doorway: true,
            stroke,
            ..inputs(shell, flow, picker_open, editor_focus)
        }
    }

    /// D9 — the concatenation: the synthetic depth-3 chain
    /// `[root, TrackEntries, TrackEntryForm, EntryDatePicker]` reads
    /// the same head as the flat single-level mask — the silent
    /// children append nothing and the walk stops at the first
    /// masking head.
    #[test]
    fn the_scope_stack_concatenates_the_chain_stopping_at_the_first_mask() {
        use crate::surfaces::{FlowTag, Surface, Surfaces};
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut chain = Surfaces::rooted(crate::view_state::ViewId::TaskDetail);
        chain
            .push(Surface::Flow(Flow::TrackEntries { task }))
            .expect("push");
        chain
            .push(Surface::Flow(Flow::TrackEntryForm { task, seed: None }))
            .expect("push");
        chain
            .push(Surface::Flow(Flow::EntryDatePicker))
            .expect("push");
        let tags: Vec<FlowTag> = chain
            .above_levels()
            .iter()
            .rev()
            .filter_map(|level| level.flow().map(crate::view_state::Flow::tag))
            .collect();
        let shell = KeymapShell {
            track_entries: true,
            ..KeymapShell::default()
        };
        let no_flow = Flow::None;
        let i = ScopeInputs {
            shell: &shell,
            picker_open: false,
            editor_focus: false,
            flow: &no_flow,
            chain: &tags,
            stroke: None,
            at_key_doorway: false,
        };
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::TrackEntries,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );
    }

    /// T-S1 — the port checklist (plan §4): every legacy chain shape
    /// byte-equal between `scope_stack` and the retired
    /// `KeymapShell::chain` (the deltas' arming facts off). The
    /// checklist's nine rows are the contract's length.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn the_port_checklist_matches_the_legacy_chain_byte_for_byte() {
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let no_flow = Flow::None;
        let edit_date = Flow::EditDate {
            task,
            field: DateField::Due,
        };

        // 1. menu open → [Menu].
        let shell = KeymapShell {
            menu: crate::shell::MenuState {
                open: true,
                ..crate::shell::MenuState::default()
            },
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(scope_stack(&i), vec![ContextId::Menu]);

        // 2–5. the modal surfaces: the flow-armed chains.
        let shell = KeymapShell {
            palette: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::Palette, ContextId::ViInsert, ContextId::Global]
        );

        let shell = KeymapShell {
            focus_overlay: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::FocusMode, ContextId::ViInsert, ContextId::Global]
        );

        let shell = KeymapShell {
            track_entries: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::TrackEntries,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );

        let shell = KeymapShell {
            review_walk: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::ReviewWalk,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );

        // 6. insert + grid focus → [DatePicker, ViInsert, Global].
        let shell = KeymapShell {
            mode: Mode::Insert,
            date_picker_grid: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &edit_date, true, false);
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );

        // 7. insert, no grid → [ViInsert, Global] (the field phase's
        //    delta OFF: the legacy shape).
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &edit_date, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::ViInsert, ContextId::Global]
        );

        // 8. normal, grid focus, view Board → [DatePicker, Board,
        //    ViNormal, Global].
        let shell = KeymapShell {
            view: Some(ContextId::Board),
            date_picker_grid: true,
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &edit_date, true, false);
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::Board,
                ContextId::ViNormal,
                ContextId::Global
            ]
        );

        // 0. normal, no view → [ViNormal, Global] (the mode context
        //    is always present — D10/D11; the retired shell test's
        //    first row).
        let shell = KeymapShell::default();
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::ViNormal, ContextId::Global]
        );

        // 9. normal, view Board → [Board, ViNormal, Global].
        let shell = KeymapShell {
            view: Some(ContextId::Board),
            ..KeymapShell::default()
        };
        let i = inputs(&shell, &no_flow, false, false);
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::Board, ContextId::ViNormal, ContextId::Global]
        );
    }

    /// T-S1's delta rows: the two intentional behavioral diffs.
    #[test]
    fn the_declared_deltas_ride_the_head() {
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let edit_date = Flow::EditDate {
            task,
            field: DateField::Due,
        };

        // D-δ1: the picker's field phase arms the head non-masking —
        // `[DatePicker, ViInsert, Global]` where the legacy chain is
        // `[ViInsert, Global]` — at the key doorway, for chord-class
        // strokes only.
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let chord = crate::keymap::stroke::parse("Ctrl+p").expect("parses");
        let i = doorway(&shell, &edit_date, true, false, Some(chord));
        assert!(i.picker_field_phase());
        assert!(i.field_phase_head_arms());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );
        // The pinned exemption: bare Return/Escape keep the legacy
        // chain (the field's commit and the phase step-back).
        for spelling in ["Return", "Escape"] {
            let bare = crate::keymap::stroke::parse(spelling).expect("parses");
            let i = doorway(&shell, &edit_date, true, false, Some(bare));
            assert!(i.picker_field_phase());
            assert!(
                !i.field_phase_head_arms(),
                "{spelling} stays with the field"
            );
            assert_eq!(
                scope_stack(&i),
                vec![ContextId::ViInsert, ContextId::Global]
            );
        }
        // Off-doorway callers (refresh-pass hints) never arm it.
        let i = inputs(&shell, &edit_date, true, false);
        assert!(i.picker_field_phase());
        assert!(!i.field_phase_head_arms());
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::ViInsert, ContextId::Global]
        );
        // The grid phase keeps the legacy head (no double ride).
        let shell = KeymapShell {
            mode: Mode::Insert,
            date_picker_grid: true,
            ..KeymapShell::default()
        };
        let i = doorway(&shell, &edit_date, true, false, Some(chord));
        assert!(!i.picker_field_phase());
        assert_eq!(
            scope_stack(&i),
            vec![
                ContextId::DatePicker,
                ContextId::ViInsert,
                ContextId::Global
            ]
        );
        // A mounted picker without the EditDate flow arms nothing.
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let i = doorway(&shell, &Flow::None, true, false, Some(chord));
        assert!(!i.picker_field_phase());
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::ViInsert, ContextId::Global]
        );

        // D-δ2: the editor element scope rides the head — doorway only.
        let shell = KeymapShell {
            mode: Mode::Insert,
            ..KeymapShell::default()
        };
        let i = doorway(&shell, &Flow::None, false, true, Some(chord));
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::Editor, ContextId::ViInsert, ContextId::Global]
        );
        let i = inputs(&shell, &Flow::None, false, true);
        assert_eq!(
            scope_stack(&i),
            vec![ContextId::ViInsert, ContextId::Global],
            "off-doorway stays legacy"
        );
        // The Editor context is keymap-unbound in every preset and
        // never masks — the resolver's chain walk skips it.
        for preset in [
            crate::keymap::Preset::Standard,
            crate::keymap::Preset::Emacs,
            crate::keymap::Preset::Vi,
        ] {
            let keymap = crate::keymap::Keymap::from_preset(preset);
            assert!(!preset.masks().contains(&ContextId::Editor));
            assert!(keymap.bindings_in(ContextId::Editor).is_none());
        }
    }

    // T-S1's property arm: for arbitrary (modal, mode, grid, view,
    // picker, editor-focus, doorway, stroke) tuples, `scope_stack`
    // follows the ordering specification — the legacy shapes plus
    // exactly the two declared deltas.
    proptest! {
        #[test]
        fn scope_stack_follows_the_ordering_specification(
            menu in proptest::bool::ANY,
            palette in proptest::bool::ANY,
            focus_overlay in proptest::bool::ANY,
            track_entries in proptest::bool::ANY,
            review_walk in proptest::bool::ANY,
            insert in proptest::bool::ANY,
            grid in proptest::bool::ANY,
            has_view in proptest::bool::ANY,
            picker_open in proptest::bool::ANY,
            editor_focus in proptest::bool::ANY,
            edit_date_flow in proptest::bool::ANY,
            at_doorway in proptest::bool::ANY,
            bare_return_escape in proptest::bool::ANY,
        ) {
            let shell = KeymapShell {
                menu: crate::shell::MenuState {
                    open: menu,
                    ..crate::shell::MenuState::default()
                },
                palette,
                focus_overlay,
                track_entries,
                review_walk,
                mode: if insert { Mode::Insert } else { Mode::Normal },
                date_picker_grid: grid,
                view: has_view.then_some(ContextId::Board),
                ..KeymapShell::default()
            };
            let task = TaskId::from_uuid(uuid::Uuid::new_v4());
            let flow = if edit_date_flow {
                Flow::EditDate { task, field: DateField::Due }
            } else {
                Flow::None
            };
            let stroke = bare_return_escape.then(|| {
                crate::keymap::stroke::parse("Return").expect("parses")
            });
            let i = ScopeInputs {
                at_key_doorway: at_doorway,
                stroke,
                ..inputs(&shell, &flow, picker_open, editor_focus)
            };
            let stack = scope_stack(&i);
            // The specification (round plan §4.7 ordering), not a
            // reimplementation: the modal surfaces are total shapes;
            // otherwise the stack is [Editor?] [DatePicker?] then the
            // mode tail — insert: [ViInsert, Global]; normal:
            // [view?, ViNormal, Global] — with the element/surface
            // heads present exactly under their arming facts.
            let modal = [
                (palette, ContextId::Palette),
                (focus_overlay, ContextId::FocusMode),
                (track_entries, ContextId::TrackEntries),
                (review_walk, ContextId::ReviewWalk),
            ]
            .into_iter()
            .find_map(|(armed, head)| armed.then_some(head));
            if menu {
                prop_assert_eq!(stack, vec![ContextId::Menu]);
            } else if let Some(head) = modal {
                prop_assert_eq!(stack, vec![head, ContextId::ViInsert, ContextId::Global]);
            } else {
                let mut expected = Vec::new();
                if at_doorway && editor_focus {
                    expected.push(ContextId::Editor);
                }
                let field_phase = at_doorway
                    && edit_date_flow
                    && picker_open
                    && !grid
                    && !bare_return_escape;
                if grid || field_phase {
                    expected.push(ContextId::DatePicker);
                }
                if insert {
                    expected.extend([ContextId::ViInsert, ContextId::Global]);
                } else {
                    if has_view {
                        expected.push(ContextId::Board);
                    }
                    expected.extend([ContextId::ViNormal, ContextId::Global]);
                }
                prop_assert_eq!(stack, expected);
            }
        }
    }
    /// T-M1 — the monopolized set: every bare printable and Space is
    /// the field's in insert mode; a chord is not; and the D20
    /// convergence: the mode gate makes the monopoly agree with the
    /// insert chain (a flowless field is Insert whenever the count is
    /// positive), while the Normal-mode gate keeps the vi editor
    /// ladder reachable.
    #[test]
    fn the_monopoly_claims_printables_and_space_in_insert() {
        let ok = |sp: &str| {
            let s = crate::keymap::stroke::parse(sp).expect("parses");
            assert!(s.is_monopolizable(), "{sp} is monopolizable");
            assert!(
                monopoly_declines(Mode::Insert, 1, false, s),
                "{sp} declines to the field"
            );
        };
        for sp in ["a", "Z", "9", "!", "Space"] {
            ok(sp);
        }
        // Chords never monopolize — even of a monopolizable key (the
        // rule gates on the modifiers, the predicate on the key).
        let chord = crate::keymap::stroke::parse("Ctrl+a").expect("parses");
        assert!(chord.is_monopolizable());
        assert!(!monopoly_declines(Mode::Insert, 1, false, chord));
        // No focus, no monopoly; no insert, no monopoly (the vi
        // Normal-mode ladder stays reachable).
        let a = crate::keymap::stroke::parse("a").expect("parses");
        assert!(!monopoly_declines(Mode::Insert, 0, false, a));
        assert!(!monopoly_declines(Mode::Normal, 1, false, a));
        // A pending keymap prefix owns the follower.
        assert!(!monopoly_declines(Mode::Insert, 1, true, a));
    }

    /// T-M2 — the chord-class set: every named non-Space key is
    /// scope-bindable (the monopoly never takes it), so Tab traversal,
    /// Return/Escape commit-discard, and vi insert's Escape survive
    /// every field.
    #[test]
    fn named_keys_stay_chord_class() {
        for sp in [
            "Return",
            "Escape",
            "Tab",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Up",
            "Down",
            "Left",
            "Right",
            "F1",
            "F12",
        ] {
            let s = crate::keymap::stroke::parse(sp).expect("parses");
            assert!(!s.is_monopolizable(), "{sp} stays chord-class");
            assert!(!monopoly_declines(Mode::Insert, 1, false, s));
        }
    }

    /// T-M3 — Backspace's fall-through: no preset binds Backspace
    /// anywhere, so the unmonopolized stroke resolves Unhandled on
    /// every chain and reaches the field natively — the same
    /// falls-through-to-native result as today.
    #[test]
    fn backspace_is_bound_nowhere_and_falls_through() {
        for preset in [
            crate::keymap::Preset::Standard,
            crate::keymap::Preset::Emacs,
            crate::keymap::Preset::Vi,
        ] {
            for (_, context, spellings) in preset.rows() {
                for sp in spellings.iter().copied() {
                    let parsed = crate::keymap::stroke::parse(sp).expect("preset rows parse");
                    assert_ne!(
                        parsed.named_name(),
                        Some("Backspace"),
                        "{preset:?} binds Backspace in {context}"
                    );
                }
            }
        }
    }

    /// T-M4 — the activation disjointness: the reservation
    /// (Return/Space for a focused activation cell) and the monopoly
    /// are disjoint by the facts — an activation stop holds
    /// `activation-focus-count > 0` with `text-focus-count == 0`, so
    /// the monopoly never fires on a reserved stroke. Pinned here on
    /// the pure rule: the reservation's Space is monopolizable ONLY
    /// when a text focus exists.
    #[test]
    fn the_reservation_and_the_monopoly_are_disjoint_by_the_facts() {
        let space = crate::keymap::stroke::parse("Space").expect("parses");
        let ret = crate::keymap::stroke::parse("Return").expect("parses");
        for stroke in [space, ret] {
            // The activation cell's fact state: zero text focus.
            assert!(!monopoly_declines(Mode::Insert, 0, false, stroke));
        }
        // And the reservation's own gate requires a bare stroke.
        let chord_space = crate::keymap::stroke::parse("Ctrl+Space").expect("parses");
        assert!(activation_reserved(true, false, false, space));
        assert!(!activation_reserved(true, false, false, chord_space));
    }
    /// T-E1 — the editor tables map the preset grammars verbatim
    /// (D-KS3): the emacs dispatch list, the vi deliberate strip, and
    /// the standard preset's empty table (native editing only).
    #[test]
    fn the_editor_tables_map_the_preset_grammars() {
        let parse = |sp: &str| crate::keymap::stroke::parse(sp).expect("parses");
        let seq = |sp: &[&str]| {
            sp.iter()
                .map(|s| parse(s))
                .collect::<Vec<crate::keymap::Stroke>>()
        };

        let emacs = editor_table(crate::keymap::Preset::Emacs);
        let expected = [
            (&["Ctrl+b"][..], EditorCommand::CursorBackward),
            (&["Ctrl+f"], EditorCommand::CursorForward),
            (&["Ctrl+p"], EditorCommand::LineUp),
            (&["Ctrl+n"], EditorCommand::LineDown),
            (&["Ctrl+a"], EditorCommand::LineStart),
            (&["Ctrl+e"], EditorCommand::LineEnd),
            (&["Ctrl+k"], EditorCommand::KillToLineEnd),
            (&["Ctrl+d"], EditorCommand::DeleteForward),
            (&["Ctrl+Space"], EditorCommand::SetMark),
            (&["Ctrl+w"], EditorCommand::CutRegion),
            (&["Alt+w"], EditorCommand::CopyRegion),
            (&["Ctrl+y"], EditorCommand::Paste),
            (&["Ctrl+c", "Ctrl+c"], EditorCommand::Commit),
            (&["Ctrl+c", "Ctrl+k"], EditorCommand::Discard),
        ];
        for (spelling, command) in expected {
            assert_eq!(emacs.exact(&seq(spelling)), Some(command));
        }
        // Escape is NOT rebound (the plan's ruling): the existing
        // ModeNormal funnel keeps it.
        assert_eq!(emacs.exact(&seq(&["Escape"])), None);

        let vi = editor_table(crate::keymap::Preset::Vi);
        let vi_expected = [
            (&["h"][..], EditorCommand::CursorBackward),
            (&["l"], EditorCommand::CursorForward),
            (&["j"], EditorCommand::LineDown),
            (&["k"], EditorCommand::LineUp),
            (&["0"], EditorCommand::LineStart),
            (&["$"], EditorCommand::LineEnd),
            (&["x"], EditorCommand::DeleteForward),
            (&["i"], EditorCommand::EnterInsert),
        ];
        for (spelling, command) in vi_expected {
            assert_eq!(vi.exact(&seq(spelling)), Some(command));
        }
        // The recorded strip: nothing more ships (no yank/register
        // grammar, no d d prefix).
        assert!(!vi.has_prefix(&seq(&["d"])));
        assert_eq!(vi.exact(&seq(&["y"])), None);

        assert_eq!(
            editor_table(crate::keymap::Preset::Standard),
            EditorTable::default()
        );
    }

    /// T-E2 — the local prefix accumulator: exact, pending, complete,
    /// abort-consumed, and pass-through claims.
    #[test]
    fn the_editor_accumulator_claims_prefixes() {
        let parse = |sp: &str| crate::keymap::stroke::parse(sp).expect("parses");
        let table = editor_table(crate::keymap::Preset::Emacs);
        let mut acc = EditorResolver::new();

        // A single-stroke entry claims outright.
        assert_eq!(
            acc.feed(parse("Ctrl+k"), &table),
            EditorClaim::Command(EditorCommand::KillToLineEnd)
        );
        // The C-c prefix parks.
        assert_eq!(acc.feed(parse("Ctrl+c"), &table), EditorClaim::Consumed);
        // Completing: C-c C-c is the commit.
        assert_eq!(
            acc.feed(parse("Ctrl+c"), &table),
            EditorClaim::Command(EditorCommand::Commit)
        );
        // The stray twin: C-c then C-k discards.
        assert_eq!(acc.feed(parse("Ctrl+c"), &table), EditorClaim::Consumed);
        assert_eq!(
            acc.feed(parse("Ctrl+k"), &table),
            EditorClaim::Command(EditorCommand::Discard)
        );
        // Abort: C-c then an unrelated chord is consumed, claims
        // nothing.
        assert_eq!(acc.feed(parse("Ctrl+c"), &table), EditorClaim::Consumed);
        assert_eq!(
            acc.feed(parse("Ctrl+x"), &table),
            EditorClaim::Consumed,
            "an aborted editor prefix is consumed, never passed"
        );
        // Pass-through: a fresh stroke outside the table's shapes.
        assert_eq!(acc.feed(parse("Ctrl+x"), &table), EditorClaim::Pass);
        assert_eq!(acc.feed(parse("Z"), &table), EditorClaim::Pass);
    }
    fn edit(
        buffer: &str,
        cursor: usize,
        mark: Option<usize>,
        clip: Option<&str>,
        command: EditorCommand,
    ) -> EditorEdit {
        apply_editor_command(
            &EditorState {
                buffer,
                cursor,
                mark,
                clipboard: clip,
            },
            command,
        )
    }

    /// T-E3 — the motions: scalar steps (multi-byte whole), logical
    /// line edges and line up/down with column clamping; motions keep
    /// the mark (the region extends).
    #[test]
    fn the_motions_move_by_scalars_and_logical_lines() {
        use EditorCommand::*;
        assert_eq!(
            edit("añb", 1, None, None, CursorForward).cursor,
            3,
            "ñ moves whole"
        );
        assert_eq!(edit("añb", 3, None, None, CursorBackward).cursor, 1);
        assert_eq!(
            edit("ab", 0, None, None, CursorBackward).cursor,
            0,
            "clamped at 0"
        );
        assert_eq!(
            edit("ab", 2, None, None, CursorForward).cursor,
            2,
            "clamped at end"
        );
        let text = "one\ntwo2\nx";
        assert_eq!(edit(text, 6, None, None, LineStart).cursor, 4);
        assert_eq!(edit(text, 5, None, None, LineEnd).cursor, 8);
        assert_eq!(edit(text, 6, None, None, LineUp).cursor, 2, "column kept");
        assert_eq!(
            edit(text, 7, None, None, LineDown).cursor,
            10,
            "column clamped to the short line"
        );
        assert_eq!(edit(text, 1, None, None, LineUp).cursor, 1, "no line above");
        assert_eq!(
            edit(text, 9, Some(2), None, LineStart).mark,
            Some(2),
            "motions keep the mark"
        );
    }

    /// T-E4 — the surgery: kill to the line end (into the slot, the
    /// newline kept), forward delete, mark/region cut and copy, paste.
    #[test]
    fn the_surgery_edits_through_the_single_slot() {
        use EditorCommand::*;
        let kill = edit("ab\ncd", 1, None, None, KillToLineEnd);
        assert_eq!(kill.text.as_deref(), Some("a\ncd"));
        assert_eq!(kill.clipboard.as_deref(), Some("b"));
        assert_eq!(
            edit("ab\ncd", 2, None, None, KillToLineEnd).text,
            None,
            "nothing at the line end"
        );

        assert_eq!(
            edit("hold", 0, None, None, DeleteForward).text.as_deref(),
            Some("old")
        );
        assert_eq!(edit("hold", 4, None, None, DeleteForward).text, None);

        let cut = edit("alpha beta", 10, Some(2), None, CutRegion);
        assert_eq!(cut.text.as_deref(), Some("al"));
        assert_eq!(cut.cursor, 2);
        assert_eq!(cut.clipboard.as_deref(), Some("pha beta"));
        assert_eq!(cut.mark, None);

        let copy = edit("prefixed", 2, Some(0), Some("old"), CopyRegion);
        assert_eq!(copy.text, None, "copy never edits");
        assert_eq!(
            copy.clipboard.as_deref(),
            Some("pr"),
            "copy overwrites the slot"
        );
        assert_eq!(copy.cursor, 2);

        assert_eq!(
            edit("ab", 1, None, None, CutRegion).text,
            None,
            "no mark, no region"
        );

        let paste = edit("al", 2, None, Some("pha beta"), Paste);
        assert_eq!(paste.text.as_deref(), Some("alpha beta"));
        assert_eq!(paste.cursor, 10);
        assert_eq!(
            edit("al", 2, None, None, Paste).text,
            None,
            "an empty slot pastes nothing"
        );
    }

    proptest! {
        // T-E5 — every outcome's cursor lands on a scalar boundary
        // inside the (new) buffer, whatever the input offsets.
        #[test]
        fn every_outcome_lands_on_a_boundary(
            buffer in "[a-zñ\\n€]{0,12}",
            cursor in 0usize..40,
            mark in proptest::option::of(0usize..40),
            command_index in 0usize..12,
        ) {
            use EditorCommand::*;
            let commands = [
                CursorBackward, CursorForward, LineUp, LineDown, LineStart,
                LineEnd, KillToLineEnd, DeleteForward, SetMark, CutRegion,
                CopyRegion, Paste,
            ];
            let out = edit(&buffer, cursor, mark, Some("ü"), commands[command_index]);
            let text = out.text.as_deref().unwrap_or(&buffer);
            prop_assert!(out.cursor <= text.len());
            prop_assert!(text.is_char_boundary(out.cursor));
            if let Some(mark) = out.mark {
                prop_assert!(mark <= text.len() && text.is_char_boundary(mark));
            }
        }

        // T-E6 — cut then paste at the same cursor restores the buffer.
        #[test]
        fn cut_then_paste_restores_the_buffer(
            buffer in "[a-zñ€]{0,12}",
            cursor in 0usize..40,
            mark in 0usize..40,
        ) {
            let cut = edit(&buffer, cursor, Some(mark), None, EditorCommand::CutRegion);
            let after = cut.text.clone().unwrap_or_else(|| buffer.clone());
            let paste = edit(&after, cut.cursor, None, cut.clipboard.as_deref(), EditorCommand::Paste);
            let restored = paste.text.unwrap_or(after);
            prop_assert_eq!(restored, buffer);
        }
    }
}

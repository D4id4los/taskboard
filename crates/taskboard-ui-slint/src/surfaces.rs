// SPDX-License-Identifier: AGPL-3.0-only

//! The surface chain (W5b D-H1/D-H5/D-H6): views and flows are levels
//! of one owned chain. `chain[0]` — the root — is always the current
//! view; flow levels and over-views (Settings, the plan-next-day
//! surface) sit above it, innermost last.
//!
//! The one removal API is [`Surfaces::truncate_to`]: closing level *i*
//! truncates the chain to *i*, so a child can never outlive its
//! parent. Level data is private; the only way a level enters the
//! chain is [`Surfaces::push`], which enforces the once-per-chain
//! kind rule (D-H7/Q6) with the typed [`AlreadyInChain`] refusal.

use crate::actions::ContextId;
use crate::view_state::{Flow, ViewId};

/// A level's surface: the root view or a flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surface {
    View(ViewId),
    Flow(Flow),
}

/// The variant identity of a flow — discriminant, not data (Q6): two
/// levels of the same variant collide even when their payloads
/// differ. Deliberately not the adapter's `FlowKind`, which collapses
/// the modals for the resolver — that is the resolver's vocabulary,
/// not identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowTag {
    None,
    Create,
    CreateStack,
    EditTitle,
    EditDate,
    EditDescription,
    Move,
    Palette,
    Capture,
    FocusMode,
    CreateBoard,
    RenameStack,
    RenameBoard,
    MoveStack,
    TrackEntries,
    TrackEntryForm,
    DragMove,
    PlanPlace,
    ReviewWalk,
}

impl Flow {
    /// The flow's chain-identity tag (D2): the one arm per variant
    /// that the once-per-chain rule reads.
    #[must_use]
    pub fn tag(&self) -> FlowTag {
        match self {
            Flow::None => FlowTag::None,
            Flow::Create { .. } => FlowTag::Create,
            Flow::CreateStack => FlowTag::CreateStack,
            Flow::EditTitle { .. } => FlowTag::EditTitle,
            Flow::EditDate { .. } => FlowTag::EditDate,
            Flow::EditDescription { .. } => FlowTag::EditDescription,
            Flow::Move { .. } => FlowTag::Move,
            Flow::Palette { .. } => FlowTag::Palette,
            Flow::Capture => FlowTag::Capture,
            Flow::FocusMode { .. } => FlowTag::FocusMode,
            Flow::CreateBoard => FlowTag::CreateBoard,
            Flow::RenameStack { .. } => FlowTag::RenameStack,
            Flow::RenameBoard { .. } => FlowTag::RenameBoard,
            Flow::MoveStack { .. } => FlowTag::MoveStack,
            Flow::TrackEntries { .. } => FlowTag::TrackEntries,
            Flow::TrackEntryForm { .. } => FlowTag::TrackEntryForm,
            Flow::DragMove { .. } => FlowTag::DragMove,
            Flow::PlanPlace { .. } => FlowTag::PlanPlace,
            Flow::ReviewWalk { .. } => FlowTag::ReviewWalk,
        }
    }
}

/// The identity of a level's surface — the once-per-chain rule's
/// vocabulary (D2). A view kind is its `ViewId`; a flow kind is its
/// [`FlowTag`], so a view and a flow can never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    View(ViewId),
    Flow(FlowTag),
}

/// Opening a surface whose kind is already somewhere in the chain is
/// refused (Q6 — Slint's non-recursive components can mount a kind
/// once per chain). Asserted by `matches!`, never by text.
#[derive(Debug, thiserror::Error)]
#[error("surface kind already in chain")]
pub struct AlreadyInChain;

/// One level of the chain. The surface is private: the only mutators
/// are [`Surfaces`]' methods (chunk 2 widens this with per-level
/// state — the private field keeps that a crate-local change).
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    surface: Surface,
}

impl Level {
    #[must_use]
    pub const fn view(&self) -> Option<ViewId> {
        match &self.surface {
            Surface::View(view) => Some(*view),
            Surface::Flow(_) => None,
        }
    }

    #[must_use]
    pub const fn flow(&self) -> Option<&Flow> {
        match &self.surface {
            Surface::View(_) => None,
            Surface::Flow(flow) => Some(flow),
        }
    }

    #[must_use]
    pub fn kind(&self) -> SurfaceKind {
        match &self.surface {
            Surface::View(view) => SurfaceKind::View(*view),
            Surface::Flow(flow) => SurfaceKind::Flow(flow.tag()),
        }
    }
}

/// The owned chain of surfaces (D-H1). The root is stored as its
/// `ViewId`, which lifts the "chain[0] is always a view" invariant
/// into the type — no chain operation can produce a flow root.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Surfaces {
    root: ViewId,
    /// The levels above the root, innermost last. `above[i]` is
    /// chain level `i + 1`.
    above: Vec<Level>,
}

impl Surfaces {
    /// A chain rooted at `view`.
    #[must_use]
    pub fn rooted(view: ViewId) -> Self {
        Self {
            root: view,
            above: Vec::new(),
        }
    }

    /// The root view — the chain's level 0.
    #[must_use]
    pub const fn root(&self) -> ViewId {
        self.root
    }

    /// The chain depth: the root plus every level above it.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.above.len() + 1
    }

    /// The kind identity of every level, root first.
    #[must_use]
    pub fn kinds(&self) -> Vec<SurfaceKind> {
        let mut kinds = Vec::with_capacity(self.depth());
        kinds.push(SurfaceKind::View(self.root));
        kinds.extend(self.above.iter().map(Level::kind));
        kinds
    }

    /// Appends `surface` as the new innermost level and returns its
    /// chain index — the one fallible minting constructor. Refused
    /// with [`AlreadyInChain`] when the surface's kind is already in
    /// the chain (Q6).
    ///
    /// # Errors
    ///
    /// [`AlreadyInChain`] when the surface's kind is already in the
    /// chain.
    pub fn push(&mut self, surface: Surface) -> Result<usize, AlreadyInChain> {
        let kind = match &surface {
            Surface::View(view) => SurfaceKind::View(*view),
            Surface::Flow(flow) => SurfaceKind::Flow(flow.tag()),
        };
        if self.kinds().contains(&kind) {
            return Err(AlreadyInChain);
        }
        self.above.push(Level { surface });
        Ok(self.above.len())
    }

    /// Truncates the chain to `depth` — the sole removal API. Closing
    /// level *i* is `truncate_to(i)`: everything above *i* — the
    /// level's descendants — goes with it, so a child can never
    /// outlive its parent. The root (depth 1) always survives.
    pub fn truncate_to(&mut self, depth: usize) {
        self.above.truncate(depth.saturating_sub(1));
    }

    /// Swaps the root view (navigation's Replace primitive, D3): a
    /// view transition *replaces* the view strand, so every pushed
    /// view level above the root goes with the swap, while flow
    /// levels above them survive — a flow opened over an over-view
    /// outlives the view being replaced underneath it, exactly as the
    /// flat model's independent `view`/`flow` slots did.
    pub fn replace_root(&mut self, view: ViewId) {
        self.above.retain(|level| level.flow().is_some());
        self.root = view;
    }

    /// The topmost view level's `ViewId` — the flat model's `view`
    /// read (an over-view pushed over the root reads the over-view).
    #[must_use]
    pub fn top_view(&self) -> ViewId {
        self.above
            .iter()
            .rev()
            .find_map(Level::view)
            .unwrap_or(self.root)
    }

    /// Whether a view level sits above the root (Settings / the
    /// planner — the cycle guard's and the switcher's pushed fact).
    #[must_use]
    pub fn has_pushed_view(&self) -> bool {
        self.above.iter().any(|level| level.view().is_some())
    }

    /// The innermost flow level's chain index, if any.
    #[must_use]
    pub fn top_flow_index(&self) -> Option<usize> {
        self.above
            .iter()
            .rposition(|level| level.flow().is_some())
            .map(|i| i + 1)
    }

    /// The innermost flow level's `Flow`, if any — the flat model's
    /// `flow` read.
    #[must_use]
    pub fn top_flow(&self) -> Option<&Flow> {
        self.above.iter().rev().find_map(|level| level.flow())
    }

    /// The innermost flow level's write seam (the cursor-mutation
    /// paths); `None` when no flow level is live.
    pub fn top_flow_mut(&mut self) -> Option<&mut Flow> {
        self.above
            .iter_mut()
            .rev()
            .find_map(|level| match &mut level.surface {
                Surface::Flow(flow) => Some(flow),
                Surface::View(_) => None,
            })
    }

    /// Ends the innermost flow level: truncates to its index, so
    /// everything above it goes with it (there is nothing above the
    /// innermost level but more of its own subtree). No-op without a
    /// flow level.
    pub fn end_top_flow(&mut self) {
        if let Some(index) = self.top_flow_index() {
            self.truncate_to(index);
        }
    }

    /// The offer walk (D-H5/D4): the effect is offered to the levels
    /// innermost-first, and the first level whose handler answers
    /// wins. Chunk 1 has no `Bubble` variant — a handler that answers
    /// `None` bubbles, and the adapter's root handler is total, so
    /// the walk always terminates (chunk 3 adds the exit protocol's
    /// parent arm subsets at this one seam).
    #[must_use]
    pub fn offer<O>(&self, mut handle: impl FnMut(&Level) -> Option<O>) -> Option<O> {
        for level in self.above.iter().rev() {
            if let Some(answer) = handle(level) {
                return Some(answer);
            }
        }
        let root = Level {
            surface: Surface::View(self.root),
        };
        handle(&root)
    }

    /// The foreign settle fold (D-H4): settles every flow level
    /// innermost-first, each by its own kind rule — `close` settles
    /// one level's `Flow`, and the fold ends that level (truncate to
    /// its index) so the walk always advances; a rule that already
    /// ended its level makes this a no-op. View levels are never
    /// settled; a chain with no flow levels folds to empty without
    /// calling `close`.
    pub fn settle_fold<O>(&mut self, mut close: impl FnMut(&Flow) -> Vec<O>) -> Vec<O> {
        let mut outcomes = Vec::new();
        while let Some(index) = self.top_flow_index() {
            let Some(flow) = self.above[index - 1].flow().cloned() else {
                break;
            };
            outcomes.extend(close(&flow));
            self.truncate_to(index);
        }
        outcomes
    }
}

impl FlowTag {
    /// The masking modal's declared key-scope head (D-H6): the
    /// contexts the kind arms its scope stack with, innermost first —
    /// the five hard-coded total-mask branches, now data. The
    /// non-modal tags declare nothing (empty slice).
    #[must_use]
    pub fn scope_head(self) -> &'static [ContextId] {
        match self {
            FlowTag::Palette => &[ContextId::Palette, ContextId::ViInsert, ContextId::Global],
            FlowTag::FocusMode => &[ContextId::FocusMode, ContextId::ViInsert, ContextId::Global],
            FlowTag::TrackEntries | FlowTag::TrackEntryForm => &[
                ContextId::TrackEntries,
                ContextId::ViInsert,
                ContextId::Global,
            ],
            FlowTag::ReviewWalk => &[
                ContextId::ReviewWalk,
                ContextId::ViInsert,
                ContextId::Global,
            ],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_state::{DateField, WalkStage};
    use chrono::NaiveDate;
    use proptest::prelude::*;
    use taskboard_domain::{BoardId, ReviewCadence, StackId, TaskId};

    const STACK: StackId = StackId::from_uuid(uuid::Uuid::from_u128(1));
    const TASK: TaskId = TaskId::from_uuid(uuid::Uuid::from_u128(2));
    const BOARD: BoardId = BoardId::from_uuid(uuid::Uuid::from_u128(3));

    fn reference() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("a valid date")
    }

    /// One `Flow` factory per tag family (fixed ids — the chain's
    /// identity is the tag, not the data).
    fn flow_of(tag: FlowTag) -> Flow {
        match tag {
            FlowTag::None => Flow::None,
            FlowTag::Create => Flow::Create { stack: STACK },
            FlowTag::CreateStack => Flow::CreateStack,
            FlowTag::EditTitle => Flow::EditTitle { task: TASK },
            FlowTag::EditDate => Flow::EditDate {
                task: TASK,
                field: DateField::Due,
            },
            FlowTag::EditDescription => Flow::EditDescription { task: TASK },
            FlowTag::Move => Flow::Move {
                task: TASK,
                target: STACK,
            },
            FlowTag::Palette => Flow::Palette { selected: 0 },
            FlowTag::Capture => Flow::Capture,
            FlowTag::FocusMode => Flow::FocusMode { selected: 0 },
            FlowTag::CreateBoard => Flow::CreateBoard,
            FlowTag::RenameStack => Flow::RenameStack { stack: STACK },
            FlowTag::RenameBoard => Flow::RenameBoard { board: BOARD },
            FlowTag::MoveStack => Flow::MoveStack {
                stack: STACK,
                slot: 0,
            },
            FlowTag::TrackEntries => Flow::TrackEntries { task: TASK },
            FlowTag::TrackEntryForm => Flow::TrackEntryForm {
                task: TASK,
                seed: None,
            },
            FlowTag::DragMove => Flow::DragMove {
                task: TASK,
                target: STACK,
            },
            FlowTag::PlanPlace => Flow::PlanPlace { task: TASK },
            FlowTag::ReviewWalk => Flow::ReviewWalk {
                cadence: ReviewCadence::Day,
                reference: reference(),
                stage: WalkStage::Prompts,
                cursor: 0,
            },
        }
    }

    /// The tags the strategy draws from — the real flow families.
    const TAGS: &[FlowTag] = &[
        FlowTag::Create,
        FlowTag::CreateStack,
        FlowTag::EditTitle,
        FlowTag::EditDate,
        FlowTag::EditDescription,
        FlowTag::Move,
        FlowTag::Palette,
        FlowTag::Capture,
        FlowTag::FocusMode,
        FlowTag::CreateBoard,
        FlowTag::RenameStack,
        FlowTag::RenameBoard,
        FlowTag::MoveStack,
        FlowTag::TrackEntries,
        FlowTag::TrackEntryForm,
        FlowTag::DragMove,
        FlowTag::PlanPlace,
        FlowTag::ReviewWalk,
    ];

    fn tag_strategy() -> impl Strategy<Value = FlowTag> {
        prop::sample::select(TAGS.to_vec())
    }

    fn view_strategy() -> impl Strategy<Value = ViewId> {
        prop_oneof![
            Just(ViewId::Board),
            Just(ViewId::Today),
            Just(ViewId::StackDetail),
            Just(ViewId::TaskDetail),
            Just(ViewId::Review),
            Just(ViewId::Settings),
            Just(ViewId::PlanNextDay),
        ]
    }

    /// One command of the random chain-driving sequence.
    #[derive(Debug, Clone)]
    enum Command {
        /// Push the flow of the tag — exercising both the fresh push
        /// and the `AlreadyInChain` refusal when the kind is live.
        PushFlow(FlowTag),
        /// Push an over-view level.
        PushView(ViewId),
        /// Truncate to a random depth (the close-at-i primitive;
        /// clamped by the chain itself).
        TruncateTo(usize),
        /// The navigation Replace: swap the root view.
        ReplaceRoot(ViewId),
        /// End the innermost flow level (the flat model's flow
        /// clearing).
        EndTopFlow,
        /// The foreign settle fold, with a recording closer.
        ForeignSettle,
        /// The offer walk with a visit recorder.
        Offer,
    }

    fn command_strategy() -> impl Strategy<Value = Command> {
        prop_oneof![
            3 => tag_strategy().prop_map(Command::PushFlow),
            1 => view_strategy().prop_map(Command::PushView),
            1 => (1usize..6).prop_map(Command::TruncateTo),
            1 => view_strategy().prop_map(Command::ReplaceRoot),
            1 => Just(Command::EndTopFlow),
            2 => Just(Command::ForeignSettle),
            2 => Just(Command::Offer),
        ]
    }

    /// The shadow model: the kind sequence the chain must equal after
    /// every command. It mirrors the same rules independently, so a
    /// divergence means the chain did something no sequence of the
    /// primitives can explain.
    #[derive(Default)]
    struct Shadow {
        root: ViewId,
        above: Vec<SurfaceKind>,
    }

    impl Shadow {
        fn kinds(&self) -> Vec<SurfaceKind> {
            let mut kinds = vec![SurfaceKind::View(self.root)];
            kinds.extend(self.above.iter().copied());
            kinds
        }

        fn apply(
            &mut self,
            command: &Command,
            settled: &mut Vec<FlowTag>,
            visits: &mut Vec<usize>,
        ) {
            match *command {
                Command::PushFlow(tag) => {
                    let kind = SurfaceKind::Flow(tag);
                    if !self.kinds().contains(&kind) {
                        self.above.push(kind);
                    }
                }
                Command::PushView(view) => {
                    let kind = SurfaceKind::View(view);
                    if !self.kinds().contains(&kind) {
                        self.above.push(kind);
                    }
                }
                Command::TruncateTo(depth) => {
                    self.above.truncate(depth.saturating_sub(1));
                }
                Command::ReplaceRoot(view) => {
                    self.above.retain(|k| matches!(k, SurfaceKind::Flow(_)));
                    self.root = view;
                }
                Command::EndTopFlow => {
                    if let Some(i) = self
                        .above
                        .iter()
                        .rposition(|k| matches!(k, SurfaceKind::Flow(_)))
                    {
                        self.above.truncate(i);
                    }
                }
                Command::ForeignSettle => {
                    // The recording closer: settles innermost-first,
                    // ending each level by truncating to its index.
                    while let Some(i) = self
                        .above
                        .iter()
                        .rposition(|k| matches!(k, SurfaceKind::Flow(_)))
                    {
                        if let SurfaceKind::Flow(tag) = self.above[i] {
                            settled.push(tag);
                        }
                        self.above.truncate(i);
                    }
                }
                Command::Offer => {
                    // The visit recorder: the walk goes innermost
                    // first and stops at the first answer.
                    visits.clear();
                    for index in (0..=self.above.len()).rev() {
                        visits.push(index);
                        let responds =
                            index == 0 || matches!(self.above[index - 1], SurfaceKind::Flow(_));
                        if responds {
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Asserts the chain's kind sequence equals the shadow's — and
    /// therefore every structural invariant the type enforces holds
    /// after an arbitrary command sequence: the root is always a
    /// view, every level the shadow still holds is present, and no
    /// level the shadow dropped survived (a child never outlives its
    /// parent).
    fn assert_matches(chain: &Surfaces, shadow: &Shadow) -> Result<(), TestCaseError> {
        prop_assert_eq!(chain.kinds(), shadow.kinds());
        prop_assert!(chain.depth() >= 1);
        prop_assert_eq!(chain.root(), shadow.root);
        Ok(())
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(512))]

        #[test]
        fn the_chain_matches_the_shadow_under_random_commands(
            commands in prop::collection::vec(command_strategy(), 0..40),
        ) {
            let mut chain = Surfaces::rooted(ViewId::Board);
            let mut shadow = Shadow::default();
            let mut settled: Vec<FlowTag> = Vec::new();
            let mut visits: Vec<usize> = Vec::new();
            let mut real_visits: Vec<SurfaceKind> = Vec::new();
            for command in &commands {
                match command {
                    Command::PushFlow(tag) => {
                        let result = chain.push(Surface::Flow(flow_of(*tag)));
                        let fresh = !shadow.kinds()
                            .contains(&SurfaceKind::Flow(*tag));
                        if fresh {
                            prop_assert!(result.is_ok());
                        } else {
                            prop_assert!(matches!(result, Err(AlreadyInChain)));
                        }
                    }
                    Command::PushView(view) => {
                        let result = chain.push(Surface::View(*view));
                        let fresh = !shadow.kinds().contains(&SurfaceKind::View(*view));
                        if fresh {
                            prop_assert!(result.is_ok());
                        } else {
                            prop_assert!(matches!(result, Err(AlreadyInChain)));
                        }
                    }
                    Command::TruncateTo(depth) => chain.truncate_to(*depth),
                    Command::ReplaceRoot(view) => chain.replace_root(*view),
                    Command::EndTopFlow => chain.end_top_flow(),
                    Command::ForeignSettle => {
                        let folded = chain.settle_fold(|flow| vec![flow.tag()]);
                        // The fold's outcomes concatenate innermost-first:
                        // the recorded order is the reversed flow order.
                        let mut expected = shadow
                            .above
                            .iter()
                            .filter_map(|k| match k {
                                SurfaceKind::Flow(tag) => Some(*tag),
                                SurfaceKind::View(_) => None,
                            })
                            .collect::<Vec<_>>();
                        expected.reverse();
                        prop_assert_eq!(&folded, &expected);
                        settled.extend(folded);
                    }
                    Command::Offer => {
                        // The real walk with the visit recorder.
                        real_visits.clear();
                        let _ = chain.offer(|level| {
                            real_visits.push(level.kind());
                            let responds = match level.kind() {
                                SurfaceKind::Flow(_) => true,
                                SurfaceKind::View(view) => {
                                    view == chain.root() && !chain.has_pushed_view()
                                }
                            };
                            responds.then_some(())
                        });
                    }
                }
                shadow.apply(command, &mut settled, &mut visits);
                if matches!(command, Command::Offer) {
                    // The real walk must visit innermost-first, stop
                    // at the first responding level, and that
                    // responder is the innermost flow level (the root
                    // when no flow level exists) — identical to the
                    // shadow walk.
                    prop_assert!(!real_visits.is_empty());
                    prop_assert_eq!(real_visits.len(), visits.len());
                    // A Handled effect never reaches an outer level
                    // (D11 invariant 4): the walk is innermost-first
                    // and stops at the first responding level — the
                    // recorded visits are strictly descending and end
                    // at the responder.
                    let responder = visits
                        .last()
                        .copied()
                        .expect("the walk always records the responder");
                    prop_assert!(visits.windows(2).all(|w| w[0] > w[1]));
                    prop_assert!(visits.iter().all(|v| *v >= responder));
                    let flow_at = |index: usize| {
                        index > 0
                            && matches!(
                                shadow.above[index - 1],
                                SurfaceKind::Flow(_)
                            )
                    };
                    prop_assert!(responder == 0 || flow_at(responder));
                }
                assert_matches(&chain, &shadow)?;
            }
        }

        #[test]
        fn a_foreign_settle_is_idempotent(
            commands in prop::collection::vec(command_strategy(), 0..40),
        ) {
            let mut chain = Surfaces::rooted(ViewId::Board);
            for command in &commands {
                match command {
                    Command::PushFlow(tag) => {
                        let _ = chain.push(Surface::Flow(flow_of(*tag)));
                    }
                    Command::PushView(view) => {
                        let _ = chain.push(Surface::View(*view));
                    }
                    Command::TruncateTo(depth) => chain.truncate_to(*depth),
                    Command::ReplaceRoot(view) => chain.replace_root(*view),
                    Command::EndTopFlow => chain.end_top_flow(),
                    Command::ForeignSettle | Command::Offer => {}
                }
            }
            let _ = chain.settle_fold(|flow| vec![flow.tag()]);
            let after_first = chain.clone();
            // The second fold settles nothing and changes nothing.
            let second = chain.settle_fold(|_flow| vec![FlowTag::None]);
            prop_assert!(second.is_empty());
            prop_assert_eq!(&chain, &after_first);
        }
    }

    #[test]
    fn a_push_returns_the_new_level_index() {
        let mut chain = Surfaces::rooted(ViewId::Board);
        let index = chain
            .push(Surface::Flow(Flow::Capture))
            .expect("a fresh kind pushes");
        assert_eq!(index, chain.depth() - 1);
        assert_eq!(chain.depth(), 2);
    }

    #[test]
    fn a_duplicate_kind_push_is_refused_and_the_chain_is_untouched() {
        let mut chain = Surfaces::rooted(ViewId::Board);
        chain
            .push(Surface::Flow(Flow::Capture))
            .expect("a fresh kind pushes");
        let before = chain.clone();
        let result = chain.push(Surface::Flow(Flow::Capture));
        assert!(matches!(result, Err(AlreadyInChain)));
        assert_eq!(chain, before);
    }

    #[test]
    fn a_pushed_view_collides_with_its_own_kind_but_not_with_flows() {
        let mut chain = Surfaces::rooted(ViewId::Board);
        chain
            .push(Surface::View(ViewId::Settings))
            .expect("a fresh view kind pushes");
        assert!(matches!(
            chain.push(Surface::View(ViewId::Settings)),
            Err(AlreadyInChain)
        ));
        // The root's own kind is in the chain, so re-rooting the same
        // view as a pushed level is the refusal too.
        assert!(matches!(
            chain.push(Surface::View(ViewId::Board)),
            Err(AlreadyInChain)
        ));
        chain
            .push(Surface::Flow(Flow::Capture))
            .expect("a flow kind never collides with a view kind");
        assert_eq!(chain.depth(), 3);
    }

    #[test]
    fn truncation_to_a_level_takes_its_subtree_with_it() {
        let mut chain = Surfaces::rooted(ViewId::Board);
        chain.push(Surface::View(ViewId::Settings)).expect("push");
        chain.push(Surface::Flow(Flow::Capture)).expect("push");
        // Closing level 1 (the over-view) must take the level-2 flow.
        chain.truncate_to(1);
        assert_eq!(chain.depth(), 1);
        assert_eq!(chain.top_flow(), None);
        assert!(!chain.has_pushed_view());
    }

    #[test]
    fn replace_root_takes_the_pushed_views_and_leaves_the_flows() {
        let mut chain = Surfaces::rooted(ViewId::Today);
        chain.push(Surface::View(ViewId::Settings)).expect("push");
        chain.push(Surface::Flow(Flow::Capture)).expect("push");
        chain.replace_root(ViewId::Board);
        assert_eq!(chain.root(), ViewId::Board);
        assert_eq!(chain.top_view(), ViewId::Board);
        assert!(!chain.has_pushed_view());
        assert_eq!(chain.top_flow(), Some(&Flow::Capture));
        assert_eq!(chain.depth(), 2);
    }

    #[test]
    fn end_top_flow_takes_everything_above_the_innermost_flow() {
        let mut chain = Surfaces::rooted(ViewId::Board);
        chain
            .push(Surface::Flow(Flow::Create { stack: STACK }))
            .expect("push");
        chain.end_top_flow();
        assert_eq!(chain.depth(), 1);
        // Without a flow level it is a no-op.
        chain.end_top_flow();
        assert_eq!(chain.depth(), 1);
    }

    #[test]
    fn the_settle_fold_on_a_multi_flow_chain_walks_innermost_first() {
        let mut chain = Surfaces::rooted(ViewId::Today);
        chain.push(Surface::Flow(Flow::Capture)).expect("push");
        chain
            .push(Surface::Flow(Flow::Palette { selected: 0 }))
            .expect("push");
        let settled = chain.settle_fold(|flow| vec![flow.tag()]);
        assert_eq!(settled, vec![FlowTag::Palette, FlowTag::Capture]);
        assert_eq!(chain.depth(), 1);
        assert_eq!(chain.root(), ViewId::Today);
    }

    #[test]
    fn the_offer_walk_visits_innermost_first_and_stops_at_the_first_answer() {
        let mut chain = Surfaces::rooted(ViewId::Board);
        chain.push(Surface::Flow(Flow::Capture)).expect("push");
        let mut visited = Vec::new();
        let answered = chain.offer(|level| {
            visited.push(level.kind());
            match level.flow() {
                Some(Flow::Capture) => Some("handled"),
                _ => None,
            }
        });
        assert_eq!(answered, Some("handled"));
        assert_eq!(visited, vec![SurfaceKind::Flow(FlowTag::Capture)]);
    }

    #[test]
    fn the_offer_walk_bubbles_from_a_pushed_view_to_the_root() {
        let mut chain = Surfaces::rooted(ViewId::TaskDetail);
        chain.push(Surface::View(ViewId::Settings)).expect("push");
        let mut visited = Vec::new();
        let answered = chain.offer(|level| {
            visited.push(level.kind());
            match level.view() {
                Some(ViewId::TaskDetail) => Some("root answered"),
                _ => None,
            }
        });
        assert_eq!(answered, Some("root answered"));
        assert_eq!(
            visited,
            vec![
                SurfaceKind::View(ViewId::Settings),
                SurfaceKind::View(ViewId::TaskDetail),
            ]
        );
    }

    #[test]
    fn scope_head_declares_the_five_masking_modals_and_nothing_else() {
        assert_eq!(
            FlowTag::Palette.scope_head(),
            &[ContextId::Palette, ContextId::ViInsert, ContextId::Global,]
        );
        assert_eq!(
            FlowTag::FocusMode.scope_head(),
            &[ContextId::FocusMode, ContextId::ViInsert, ContextId::Global,]
        );
        assert_eq!(
            FlowTag::TrackEntries.scope_head(),
            FlowTag::TrackEntryForm.scope_head()
        );
        assert_eq!(
            FlowTag::TrackEntries.scope_head()[0],
            ContextId::TrackEntries
        );
        assert_eq!(FlowTag::ReviewWalk.scope_head()[0], ContextId::ReviewWalk);
        assert_eq!(FlowTag::Capture.scope_head(), &[]);
        assert_eq!(FlowTag::None.scope_head(), &[]);
    }
}

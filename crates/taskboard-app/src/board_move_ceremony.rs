// SPDX-License-Identifier: AGPL-3.0-only
//! The Deck→files move ceremony (D-BF11, ruling Q4): four typed,
//! resumable steps. The machine's position is never stored — it is
//! derived from the binding row and the files tier, so a rerun after a
//! crash at any point picks up at the first step not yet visible in the
//! world.
//!
//! The ownership laws: a board's owner is its one `Board.remote` row, so
//! the transition is Deck → unbound → Files and two owners are not
//! representable; publication precedes retirement, and retirement
//! precedes the files binding, whose write is the commit point that
//! closes the undo window.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use taskboard_domain::messages::SyncReport;
use taskboard_domain::{BoardId, CommandOutcome, ConnectorId, RemoteBoardId, StateCommand};
use taskboard_sync_files_nextcloud::board_cycle::{
    MoveSubtreeProbe, probe_move_subtree, publish_move_floor,
};
use taskboard_sync_files_nextcloud::client::WebDavFilesTransport;
use taskboard_sync_files_nextcloud::cycle::{PublishedState, SyncContext};
use taskboard_sync_files_nextcloud::error::TransportError;

use crate::commands::CommandError;
use crate::commands::sync::SyncOutcome;
use crate::secrets::CredentialStore;

/// Where a move stands, as derived from the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoveStage {
    /// Deck-bound; no floor on the files tier.
    NotStarted,
    /// Deck-bound; this device's floor is published.
    Published,
    /// Unbound with the floor published: the undo window.
    DeckRetired,
    /// Files-bound: the commit point has passed.
    Committed,
}

/// The ceremony's typed refusals and step failures.
#[derive(Debug, thiserror::Error)]
pub enum MoveError {
    /// `--to-deck`: files→Deck is fenced by ruling Q4 (2026-11-07) — a
    /// deliberate refusal, not an absent feature.
    #[error("the files-to-Deck direction is fenced")]
    ReverseFenced,
    /// The board has neither a Deck binding nor a floor to resume from.
    #[error("the board is not bound to Deck")]
    NotDeckBound,
    /// `[files_sync]` is off, so no files tier exists to move onto.
    #[error("files sync is not enabled")]
    FilesSyncDisabled,
    /// Local edits are still queued; the ceremony never drops them.
    #[error("the board's outbox is not quiet")]
    OutboxNotQuiescent { pending: usize },
    /// Step 1's Deck cycle did not complete.
    #[error("the final Deck pull failed")]
    DeckPullFailed(#[source] Box<CommandError>),
    /// Step 1's Deck cycle failed or ran out of its wait budget.
    #[error("the final Deck pull did not complete")]
    DeckPullIncomplete,
    /// Step 2's transport failure, or a probe around it.
    #[error("publishing the board subtree failed")]
    PublishFailed(#[source] TransportError),
    /// Step 2 ran but the floor is not readable back.
    #[error("the published floor could not be confirmed")]
    PublishUnconfirmed,
    /// The target subtree carries an identity this device never wrote.
    #[error("the files subtree belongs to another device")]
    FilesSubtreeForeign,
    /// Step 3's engine refusal.
    #[error("retiring the Deck binding was refused")]
    UnbindRefused(#[source] Box<CommandError>),
    /// Step 3 would strand ops that appeared after the guard.
    #[error("retiring the Deck binding would strand ops")]
    StrandedOps { stranded: usize },
    /// Step 4's engine refusal.
    #[error("assigning the files binding was refused")]
    AssignRefused(#[source] Box<CommandError>),
    /// `--undo` after the commit point.
    #[error("the undo window closed at the commit")]
    UndoClosed,
    /// `--undo` on a board with no move at the undo stage.
    #[error("the board has no move to undo")]
    NothingToUndo,
    /// `--undo` cannot name the retired Deck board and connector.
    #[error("the retired Deck binding cannot be reconstructed")]
    UndoCoordinatesLost,
    /// `--undo`'s engine refusal.
    #[error("restoring the Deck binding was refused")]
    UndoRefused(#[source] Box<CommandError>),
}

impl MoveError {
    /// The JSON error kind.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ReverseFenced => "move-reverse-fenced",
            Self::NotDeckBound => "move-not-deck-bound",
            Self::FilesSyncDisabled => "move-files-sync-disabled",
            Self::OutboxNotQuiescent { .. } => "move-outbox-not-quiescent",
            Self::DeckPullFailed(_) => "move-deck-pull-failed",
            Self::DeckPullIncomplete => "move-deck-pull-incomplete",
            Self::PublishFailed(_) => "move-publish-failed",
            Self::PublishUnconfirmed => "move-publish-unconfirmed",
            Self::FilesSubtreeForeign => "move-files-subtree-foreign",
            Self::UnbindRefused(_) => "move-unbind-refused",
            Self::StrandedOps { .. } => "move-stranded-ops",
            Self::AssignRefused(_) => "move-assign-refused",
            Self::UndoClosed => "move-undo-closed",
            Self::NothingToUndo => "move-nothing-to-undo",
            Self::UndoCoordinatesLost => "move-undo-coordinates-lost",
            Self::UndoRefused(_) => "move-undo-refused",
        }
    }
}

/// The board's binding reduced to what the derivation reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingKind {
    Deck,
    Files,
    Unbound,
}

/// The pure position derivation.
pub(crate) fn derive_move_stage(
    binding: BindingKind,
    probe: MoveSubtreeProbe,
) -> Result<MoveStage, MoveError> {
    use MoveSubtreeProbe as P;
    match (binding, probe) {
        (BindingKind::Files, _) => Ok(MoveStage::Committed),
        (_, P::Foreign) => Err(MoveError::FilesSubtreeForeign),
        (BindingKind::Deck, P::Absent | P::Partial) => Ok(MoveStage::NotStarted),
        (BindingKind::Deck, P::Published) => Ok(MoveStage::Published),
        (BindingKind::Unbound, P::Published) => Ok(MoveStage::DeckRetired),
        (BindingKind::Unbound, P::Absent | P::Partial) => Err(MoveError::NotDeckBound),
    }
}

/// One completed ceremony step, reported in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoveStep {
    DeckPulled,
    SubtreePublished,
    DeckRetired,
    FilesAssigned,
    DeckRestored,
}

/// The verb's receipt.
#[derive(Debug, Clone, Serialize)]
pub struct MoveReceipt {
    pub board: String,
    pub stage: MoveStage,
    pub steps: Vec<MoveStep>,
}

/// The effects the machine sequences; production is [`AppWorld`].
pub(crate) trait MoveWorld {
    fn binding(&self) -> BindingKind;
    async fn probe(&self) -> Result<MoveSubtreeProbe, MoveError>;
    async fn pull_deck(&mut self) -> Result<(), MoveError>;
    async fn pending_ops(&self) -> Result<usize, MoveError>;
    async fn publish(&mut self) -> Result<(), MoveError>;
    async fn retire(&mut self) -> Result<usize, MoveError>;
    async fn assign_files(&mut self) -> Result<(), MoveError>;
    /// Whether the retired Deck coordinates can still be named.
    fn can_restore(&self) -> bool;
    async fn restore_deck(&mut self) -> Result<(), MoveError>;
}

async fn current_stage(world: &impl MoveWorld) -> Result<MoveStage, MoveError> {
    let binding = world.binding();
    if binding == BindingKind::Files {
        return derive_move_stage(binding, MoveSubtreeProbe::Absent);
    }
    derive_move_stage(binding, world.probe().await?)
}

/// Runs the ceremony from the derived position to `stop_after` (the
/// commit, outside of tests and the undo leg of the live journey).
pub(crate) async fn advance(
    world: &mut impl MoveWorld,
    stop_after: MoveStage,
) -> Result<(MoveStage, Vec<MoveStep>), MoveError> {
    let mut stage = current_stage(world).await?;
    let mut steps = Vec::new();
    if matches!(stage, MoveStage::NotStarted | MoveStage::Published) {
        world.pull_deck().await?;
        let pending = world.pending_ops().await?;
        if pending > 0 {
            return Err(MoveError::OutboxNotQuiescent { pending });
        }
        steps.push(MoveStep::DeckPulled);
    }
    if stage == MoveStage::NotStarted {
        world.publish().await?;
        if world.probe().await? != MoveSubtreeProbe::Published {
            return Err(MoveError::PublishUnconfirmed);
        }
        stage = MoveStage::Published;
        steps.push(MoveStep::SubtreePublished);
    }
    if stage == MoveStage::Published && stop_after >= MoveStage::DeckRetired {
        let stranded = world.retire().await?;
        if stranded > 0 {
            return Err(MoveError::StrandedOps { stranded });
        }
        stage = MoveStage::DeckRetired;
        steps.push(MoveStep::DeckRetired);
    }
    if stage == MoveStage::DeckRetired && stop_after >= MoveStage::Committed {
        world.assign_files().await?;
        stage = MoveStage::Committed;
        steps.push(MoveStep::FilesAssigned);
    }
    Ok((stage, steps))
}

/// Undoes a move inside the window: only `DeckRetired` qualifies.
pub(crate) async fn undo(world: &mut impl MoveWorld) -> Result<Vec<MoveStep>, MoveError> {
    let stage = match current_stage(world).await {
        Ok(stage) => stage,
        Err(MoveError::NotDeckBound) => return Err(MoveError::NothingToUndo),
        Err(err) => return Err(err),
    };
    match stage {
        MoveStage::Committed => Err(MoveError::UndoClosed),
        MoveStage::NotStarted | MoveStage::Published => Err(MoveError::NothingToUndo),
        MoveStage::DeckRetired if !world.can_restore() => Err(MoveError::UndoCoordinatesLost),
        MoveStage::DeckRetired => {
            world.restore_deck().await?;
            world.pull_deck().await?;
            Ok(vec![MoveStep::DeckRestored, MoveStep::DeckPulled])
        }
    }
}

/// The ceremony's effects on a booted core and the live files tier.
struct AppWorld<'a> {
    app: &'a crate::App,
    board: BoardId,
    files_connector: ConnectorId,
    transport: WebDavFilesTransport,
    context: SyncContext,
    timeout: Duration,
}

impl AppWorld<'_> {
    fn remote(&self) -> uuid::Uuid {
        self.board.as_uuid()
    }

    /// The retired binding, reconstructed: the row is gone but the kept
    /// descendant refs name the Deck board, and a lone Deck connector
    /// names the connector.
    fn retired_binding(&self) -> Option<(RemoteBoardId, ConnectorId)> {
        let state = self.app.engine.shared_state().load_full();
        let remote = state
            .stacks
            .values()
            .filter(|stack| stack.board == self.board)
            .find_map(|stack| stack.remote.map(|r| r.board))?;
        remote.as_deck()?;
        let specs = crate::connectors::configured_connectors(&self.app.config).ok()?;
        let [only] = specs.as_slice() else {
            return None;
        };
        Some((remote, only.name.clone()))
    }
}

impl MoveWorld for AppWorld<'_> {
    fn binding(&self) -> BindingKind {
        let state = self.app.engine.shared_state().load_full();
        match state
            .boards
            .get(&self.board)
            .and_then(|b| b.remote.as_ref())
        {
            None => BindingKind::Unbound,
            Some(binding) if binding.board.as_files().is_some() => BindingKind::Files,
            Some(_) => BindingKind::Deck,
        }
    }

    async fn probe(&self) -> Result<MoveSubtreeProbe, MoveError> {
        probe_move_subtree(
            &self.transport,
            self.context.identity.device_id,
            self.remote(),
        )
        .await
        .map_err(MoveError::PublishFailed)
    }

    async fn pull_deck(&mut self) -> Result<(), MoveError> {
        let state = self.app.engine.shared_state().load_full();
        let connector = state
            .boards
            .get(&self.board)
            .and_then(|b| b.remote.as_ref())
            .map(|binding| binding.connector.clone());
        drop(state);
        let outcome =
            crate::commands::sync::sync_on(self.app, false, self.timeout, connector.as_ref())
                .await
                .map_err(|err| MoveError::DeckPullFailed(Box::new(err)))?;
        match outcome {
            SyncOutcome::Succeeded(_) => Ok(()),
            SyncOutcome::Failed(..) | SyncOutcome::TimedOut(_) | SyncOutcome::NoWait(_) => Err(
                MoveError::DeckPullFailed(Box::new(CommandError::SyncDisabled)),
            ),
        }
    }

    async fn pending_ops(&self) -> Result<usize, MoveError> {
        crate::commands::boards::pending_ops_of(self.app, self.board)
            .await
            .map_err(|err| MoveError::DeckPullFailed(Box::new(err)))
    }

    async fn publish(&mut self) -> Result<(), MoveError> {
        let persisted = taskboard_domain::SyncStateReader::read_state(&self.app.engine)
            .await
            .map_err(|_| {
                MoveError::DeckPullFailed(Box::new(CommandError::Engine(Box::new(
                    taskboard_state::ExecuteError::EngineGone,
                ))))
            })?;
        publish_move_floor(
            &self.transport,
            &self.context,
            self.board,
            self.remote(),
            &persisted,
        )
        .await
        .map_err(MoveError::PublishFailed)
    }

    async fn retire(&mut self) -> Result<usize, MoveError> {
        let outcome = self
            .app
            .engine
            .execute(StateCommand::UnbindBoard { board: self.board })
            .await
            .map_err(|err| MoveError::UnbindRefused(Box::new(CommandError::from(err))))?;
        Ok(match outcome {
            CommandOutcome::Unbound { stranded_ops } => stranded_ops,
            _ => 0,
        })
    }

    async fn assign_files(&mut self) -> Result<(), MoveError> {
        self.app
            .engine
            .execute(StateCommand::BindBoard {
                board: self.board,
                remote: RemoteBoardId::files(self.remote()),
                connector: self.files_connector.clone(),
            })
            .await
            .map(|_| ())
            .map_err(|err| MoveError::AssignRefused(Box::new(CommandError::from(err))))
    }

    fn can_restore(&self) -> bool {
        self.retired_binding().is_some()
    }

    async fn restore_deck(&mut self) -> Result<(), MoveError> {
        let (remote, connector) = self
            .retired_binding()
            .ok_or(MoveError::UndoCoordinatesLost)?;
        self.app
            .engine
            .execute(StateCommand::BindBoard {
                board: self.board,
                remote,
                connector,
            })
            .await
            .map(|_| ())
            .map_err(|err| MoveError::UndoRefused(Box::new(CommandError::from(err))))
    }
}

/// What the verb was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    ToFiles,
    ToDeck,
    Undo,
}

/// The ceremony on a booted core; the caller owns the shutdown.
///
/// # Errors
///
/// [`CommandError::BoardMove`] for every ceremony refusal; credential,
/// client and board-resolution classes otherwise.
pub async fn run_on(
    app: &crate::App,
    store: &Arc<dyn CredentialStore>,
    board: BoardId,
    direction: MoveDirection,
    timeout: Duration,
) -> Result<MoveReceipt, CommandError> {
    run_until(app, store, board, direction, MoveStage::Committed, timeout).await
}

/// [`run_on`] with an explicit stop stage (the live journey's undo leg
/// holds the ceremony inside the window).
///
/// # Errors
///
/// As [`run_on`].
pub async fn run_until(
    app: &crate::App,
    store: &Arc<dyn CredentialStore>,
    board: BoardId,
    direction: MoveDirection,
    stop_after: MoveStage,
    timeout: Duration,
) -> Result<MoveReceipt, CommandError> {
    if direction == MoveDirection::ToDeck {
        return Err(MoveError::ReverseFenced.into());
    }
    let board = crate::commands::resolve_board(app, Some(board))?;
    let mut world = app_world(app, store, board, timeout).await?;
    let (stage, steps) = match direction {
        MoveDirection::Undo => {
            let steps = undo(&mut world).await?;
            (MoveStage::NotStarted, steps)
        }
        _ => advance(&mut world, stop_after).await?,
    };
    Ok(MoveReceipt {
        board: board.as_uuid().to_string(),
        stage,
        steps,
    })
}

async fn app_world<'a>(
    app: &'a crate::App,
    store: &Arc<dyn CredentialStore>,
    board: BoardId,
    timeout: Duration,
) -> Result<AppWorld<'a>, CommandError> {
    let spec = crate::bootstrap::files_sync_spawn_decision(&app.config)
        .ok_or(MoveError::FilesSyncDisabled)?;
    let password = store.load(&spec.server_url, &spec.username)?;
    let transport = WebDavFilesTransport::new(&spec.server_url, &spec.username, password.expose())
        .map_err(MoveError::PublishFailed)?;
    let clock: Arc<dyn taskboard_domain::Clock> = Arc::new(taskboard_domain::SystemClock);
    let device_json = dirs::data_dir()
        .ok_or(MoveError::FilesSyncDisabled)?
        .join("taskboard")
        .join("device.json");
    let identity =
        taskboard_sync_files_nextcloud::device::load_or_create(&device_json, clock.as_ref())
            .await
            .map_err(|_| MoveError::FilesSyncDisabled)?;
    let context = SyncContext {
        identity,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        clock,
        local: Arc::new(PublishedState::new(app.engine.shared_state())),
        report_tx: tokio::sync::mpsc::channel(1).0,
        connector: spec.identity.clone(),
        state_reader: Arc::new(app.engine.clone()),
        board_reports: tokio::sync::mpsc::channel::<SyncReport>(1).0,
    };
    Ok(AppWorld {
        app,
        board,
        files_connector: spec.identity,
        transport,
        context,
        timeout,
    })
}

/// The verb's text rendering: one line per completed step, then the
/// terminal line.
#[must_use]
pub fn render_text(receipt: &MoveReceipt) -> String {
    let total = 4;
    let mut lines: Vec<String> = receipt
        .steps
        .iter()
        .map(|step| match step {
            MoveStep::DeckPulled => {
                format!("move: step 1/{total} done: final Deck pull, outbox quiet")
            }
            MoveStep::SubtreePublished => {
                format!("move: step 2/{total} done: board subtree published")
            }
            MoveStep::DeckRetired => {
                format!(
                    "move: step 3/{total} done: Deck binding retired (undoable until the next step)"
                )
            }
            MoveStep::FilesAssigned => {
                format!("move: step 4/{total} done: files binding committed")
            }
            MoveStep::DeckRestored => "move: undo: Deck binding restored".to_owned(),
        })
        .collect();
    lines.push(match receipt.stage {
        MoveStage::Committed => format!("board {} now syncs over files", receipt.board),
        MoveStage::DeckRetired => {
            format!(
                "board {} is retired from Deck and not yet committed",
                receipt.board
            )
        }
        MoveStage::Published | MoveStage::NotStarted => {
            format!("board {} is bound to Deck", receipt.board)
        }
    });
    lines.join("\n")
}

#[cfg(test)]
#[allow(clippy::unused_async_trait_impl, clippy::struct_excessive_bools)] // a synchronous fake of an async seam
mod tests {
    use super::*;

    /// A scripted world: every binding write is logged, and `fail_*`
    /// models a process dying at that seam.
    #[derive(Default)]
    struct Fake {
        unbound: bool,
        files: bool,
        probe: Option<MoveSubtreeProbe>,
        pending: usize,
        stranded: usize,
        restorable: bool,
        die_in_publish: bool,
        die_in_assign: bool,
        log: Vec<&'static str>,
    }

    impl Fake {
        fn deck() -> Self {
            Self {
                restorable: true,
                ..Self::default()
            }
        }

        fn end_state(&self) -> (BindingKind, MoveSubtreeProbe) {
            (
                self.binding(),
                self.probe.unwrap_or(MoveSubtreeProbe::Absent),
            )
        }
    }

    impl MoveWorld for Fake {
        fn binding(&self) -> BindingKind {
            match (self.files, self.unbound) {
                (true, _) => BindingKind::Files,
                (false, true) => BindingKind::Unbound,
                (false, false) => BindingKind::Deck,
            }
        }
        async fn probe(&self) -> Result<MoveSubtreeProbe, MoveError> {
            Ok(self.probe.unwrap_or(MoveSubtreeProbe::Absent))
        }
        async fn pull_deck(&mut self) -> Result<(), MoveError> {
            self.log.push("pull");
            Ok(())
        }
        async fn pending_ops(&self) -> Result<usize, MoveError> {
            Ok(self.pending)
        }
        async fn publish(&mut self) -> Result<(), MoveError> {
            self.log.push("publish");
            if self.die_in_publish {
                self.probe = Some(MoveSubtreeProbe::Partial);
                self.die_in_publish = false;
                return Err(MoveError::PublishUnconfirmed);
            }
            self.probe = Some(MoveSubtreeProbe::Published);
            Ok(())
        }
        async fn retire(&mut self) -> Result<usize, MoveError> {
            assert!(!self.files, "a files binding predates retirement");
            self.log.push("retire");
            self.unbound = true;
            Ok(self.stranded)
        }
        async fn assign_files(&mut self) -> Result<(), MoveError> {
            assert!(
                self.unbound,
                "two owners: the files binding before retirement"
            );
            self.log.push("assign");
            if self.die_in_assign {
                self.die_in_assign = false;
                return Err(MoveError::PublishUnconfirmed);
            }
            self.files = true;
            Ok(())
        }
        fn can_restore(&self) -> bool {
            self.restorable
        }
        async fn restore_deck(&mut self) -> Result<(), MoveError> {
            self.log.push("restore");
            self.unbound = false;
            Ok(())
        }
    }

    const COMMITTED: (BindingKind, MoveSubtreeProbe) =
        (BindingKind::Files, MoveSubtreeProbe::Published);

    #[test]
    fn the_stage_is_a_function_of_the_world() {
        use BindingKind::{Deck, Files, Unbound};
        use MoveSubtreeProbe as P;
        assert_eq!(
            derive_move_stage(Deck, P::Absent).ok(),
            Some(MoveStage::NotStarted)
        );
        assert_eq!(
            derive_move_stage(Deck, P::Partial).ok(),
            Some(MoveStage::NotStarted)
        );
        assert_eq!(
            derive_move_stage(Deck, P::Published).ok(),
            Some(MoveStage::Published)
        );
        assert_eq!(
            derive_move_stage(Unbound, P::Published).ok(),
            Some(MoveStage::DeckRetired)
        );
        assert_eq!(
            derive_move_stage(Files, P::Absent).ok(),
            Some(MoveStage::Committed)
        );
        assert!(matches!(
            derive_move_stage(Unbound, P::Absent),
            Err(MoveError::NotDeckBound)
        ));
        assert!(matches!(
            derive_move_stage(Deck, P::Foreign),
            Err(MoveError::FilesSubtreeForeign)
        ));
    }

    #[tokio::test]
    async fn the_full_run_publishes_then_retires_then_assigns() {
        let mut world = Fake::deck();
        let (stage, _) = advance(&mut world, MoveStage::Committed)
            .await
            .expect("moves");
        assert_eq!(stage, MoveStage::Committed);
        assert_eq!(world.log, ["pull", "publish", "retire", "assign"]);
        assert_eq!(world.end_state(), COMMITTED);
    }

    #[tokio::test]
    async fn a_busy_outbox_writes_nothing() {
        let mut world = Fake {
            pending: 3,
            ..Fake::deck()
        };
        let refused = advance(&mut world, MoveStage::Committed).await;
        assert!(matches!(
            refused,
            Err(MoveError::OutboxNotQuiescent { pending: 3 })
        ));
        assert_eq!(world.log, ["pull"]);
        assert_eq!(world.binding(), BindingKind::Deck);
    }

    #[tokio::test]
    async fn a_foreign_subtree_refuses_before_any_effect() {
        let mut world = Fake {
            probe: Some(MoveSubtreeProbe::Foreign),
            ..Fake::deck()
        };
        let refused = advance(&mut world, MoveStage::Committed).await;
        assert!(matches!(refused, Err(MoveError::FilesSubtreeForeign)));
        assert_eq!(world.log, [] as [&str; 0]);
    }

    #[tokio::test]
    async fn stranded_ops_at_retirement_stop_before_the_commit() {
        let mut world = Fake {
            stranded: 2,
            ..Fake::deck()
        };
        let refused = advance(&mut world, MoveStage::Committed).await;
        assert!(matches!(
            refused,
            Err(MoveError::StrandedOps { stranded: 2 })
        ));
        assert!(!world.files);
    }

    #[tokio::test]
    async fn a_death_mid_publish_resumes_to_the_same_end() {
        let mut world = Fake {
            die_in_publish: true,
            ..Fake::deck()
        };
        assert!(advance(&mut world, MoveStage::Committed).await.is_err());
        assert_eq!(world.binding(), BindingKind::Deck);
        advance(&mut world, MoveStage::Committed)
            .await
            .expect("resumes");
        assert_eq!(world.end_state(), COMMITTED);
    }

    #[tokio::test]
    async fn a_death_after_publish_resumes_without_republishing() {
        let mut world = Fake::deck();
        advance(&mut world, MoveStage::Published)
            .await
            .expect("stops");
        assert_eq!(world.binding(), BindingKind::Deck);
        world.log.clear();
        advance(&mut world, MoveStage::Committed)
            .await
            .expect("resumes");
        assert_eq!(world.log, ["pull", "retire", "assign"]);
        assert_eq!(world.end_state(), COMMITTED);
    }

    #[tokio::test]
    async fn a_death_between_retire_and_assign_resumes_at_the_commit() {
        let mut world = Fake {
            die_in_assign: true,
            ..Fake::deck()
        };
        assert!(advance(&mut world, MoveStage::Committed).await.is_err());
        assert_eq!(world.binding(), BindingKind::Unbound);
        world.log.clear();
        advance(&mut world, MoveStage::Committed)
            .await
            .expect("resumes");
        assert_eq!(world.log, ["assign"]);
        assert_eq!(world.end_state(), COMMITTED);
    }

    #[tokio::test]
    async fn a_rerun_after_the_commit_is_a_no_op() {
        let mut world = Fake::deck();
        advance(&mut world, MoveStage::Committed)
            .await
            .expect("moves");
        world.log.clear();
        let (stage, steps) = advance(&mut world, MoveStage::Committed)
            .await
            .expect("reruns");
        assert_eq!(stage, MoveStage::Committed);
        assert_eq!(steps, [] as [MoveStep; 0]);
        assert_eq!(world.log, [] as [&str; 0]);
    }

    #[tokio::test]
    async fn undo_inside_the_window_restores_deck_and_pulls() {
        let mut world = Fake::deck();
        advance(&mut world, MoveStage::DeckRetired)
            .await
            .expect("stops");
        world.log.clear();
        undo(&mut world).await.expect("undoes");
        assert_eq!(world.log, ["restore", "pull"]);
        assert_eq!(world.binding(), BindingKind::Deck);
    }

    #[tokio::test]
    async fn undo_after_the_commit_refuses() {
        let mut world = Fake::deck();
        advance(&mut world, MoveStage::Committed)
            .await
            .expect("moves");
        assert!(matches!(undo(&mut world).await, Err(MoveError::UndoClosed)));
        assert_eq!(world.binding(), BindingKind::Files);
    }

    #[tokio::test]
    async fn undo_without_a_move_refuses() {
        let mut bound = Fake::deck();
        assert!(matches!(
            undo(&mut bound).await,
            Err(MoveError::NothingToUndo)
        ));
        let mut loose = Fake {
            unbound: true,
            ..Fake::deck()
        };
        assert!(matches!(
            undo(&mut loose).await,
            Err(MoveError::NothingToUndo)
        ));
    }

    #[tokio::test]
    async fn undo_refuses_when_the_coordinates_are_gone() {
        let mut world = Fake {
            restorable: false,
            ..Fake::deck()
        };
        advance(&mut world, MoveStage::DeckRetired)
            .await
            .expect("stops");
        assert!(matches!(
            undo(&mut world).await,
            Err(MoveError::UndoCoordinatesLost)
        ));
        assert_eq!(world.binding(), BindingKind::Unbound);
    }
}

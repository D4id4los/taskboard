// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure domain types, entities, and inter-actor protocols for `taskboard`.
//!
//! This crate is the functional core of the system. It must remain pure:
//! zero network, zero UI, zero persistence, and zero asynchronous runtime
//! dependencies. All inter-actor communication types ([`StateCommand`]-style
//! message payloads) live here so that every other workspace crate can
//! share them without coupling to each other.
//!
//! Design rules (see `AGENTS.md`):
//! - Domain logic is expressed as pure functions: inputs in, outputs out,
//!   no hidden mutable state. Merge functions never read the clock or
//!   generate ids — time and identity are always data passed in.
//! - IO interfaces (ports, e.g. [`TaskRepository`]) are defined here as
//!   traits returning [`BoxFuture`] (no `async-trait`/`tokio`) and
//!   implemented by adapter crates (`taskboard-storage-sqlite`).
//! - The sync conflict policy ([`apply_sync_report`] and the per-entity
//!   primitives) is executed by the state engine, never by the sync actor.
//!
//! Module map: identifiers ([`ids`]), time seam ([`clock`]), identity
//! seam ([`idgen`]), entities ([`entities`]), state ([`state`]), remote
//! views ([`remote`]), outbox ([`outbox`]), persistence port
//! ([`persistence`]) with the shared batch semantics ([`apply_actions`]),
//! messages ([`messages`]), local command semantics ([`command`]),
//! conflict-policy algebra ([`merge`]) and its fixed-order composition
//! ([`pipeline`]).

#![forbid(unsafe_code)]

pub mod clock;
pub mod command;
pub mod entities;
pub mod idgen;
pub mod ids;
pub mod merge;
pub mod messages;
pub mod outbox;
pub mod persistence;
pub mod pipeline;
pub mod push;
pub mod remote;
pub mod state;

/// Compiled for our own tests (dev-dep `proptest`) and for downstream
/// crates that enable the `test-support` feature.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
pub(crate) mod merge_testutil;

pub use clock::{Clock, SystemClock};
pub use command::{CommandError, CommandOutcome, CommandPlan, plan_command};
pub use entities::{Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks};
pub use idgen::{IdGenerator, UuidV7Generator};
pub use ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
pub use merge::{
    adopt_board_if_newer, adopt_label_after_push, adopt_remote_board, adopt_remote_label,
    adopt_remote_stack, adopt_remote_task, adopt_stack_after_push, adopt_task_after_push,
    finalize_pushed_task_delete, merge_label, merge_stack, merge_task, tombstone_label,
    tombstone_stack, tombstone_task,
};
pub use messages::{
    EngineSignal, LabelChanges, StateCommand, SyncCommand, SyncReport, SystemEvent, TaskChanges,
};
pub use outbox::{LocalOp, OpId, PendingOp};
pub use persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, SyncStateReader, SyncValidators,
    TaskRepository, ValidatorKey, apply_actions,
};
pub use pipeline::{apply_push_report, apply_sync_report};
pub use push::{
    CardShape, EntityTables, MaterializedOp, NewCardShape, PushGroup, PushTarget, plan_pushes,
};
pub use remote::{
    BoardPullValidators, PushOutcome, PushResult, RemoteBoard, RemoteBoardSnapshot, RemoteEcho,
    RemoteIndex, RemoteLabel, RemoteStack, RemoteTask, resolve_label,
};
pub use state::{AppState, SyncErrorKind, SyncPhase, SyncStatus};

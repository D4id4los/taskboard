// SPDX-License-Identifier: AGPL-3.0-only
//! The UI lifecycle and adaptation layer (GUI Phase 7, chunk 1).
//!
//! `taskboard-app` owns every IO seam and hands the presentation crate
//! typed values and channel halves (design §1.2): this module adapts the
//! booted core ([`crate::bootstrap::App`]) onto the UI's hand-over set.
//! Chunk 1 carries the *UI-command forwarder* — the actor that drains
//! the UI's command channel into the state engine; the desktop entry
//! lifecycle lands in this module too.

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use taskboard_domain::StateCommand;
use taskboard_state::EngineHandle;

/// The engine-side sink the forwarder adapts onto. A trait so the
/// forwarder is hermetically testable without a state engine.
pub trait CommandSink: Send + 'static {
    /// Hands one command to the engine (fire-and-forget; drop logging
    /// on a full or closed inbox is the sink's own behaviour).
    fn dispatch(&self, command: StateCommand);
}

impl CommandSink for EngineHandle {
    fn dispatch(&self, command: StateCommand) {
        // EngineHandle::dispatch's own try_send drop logging (full or
        // closed inbox) is the state crate's tested behaviour, not
        // re-tested here (one behaviour, one test, lowest layer).
        EngineHandle::dispatch(self, command);
    }
}

/// Capacity of the UI → app command channel. The engine inbox is 256
/// deep; UI emission is human-input-scale, so 64 queued intents are
/// already an unresponsive-UI pathology rather than a throughput need.
/// (Consumed by the desktop entry lifecycle, later in this chunk; the
/// tests use it meanwhile.)
#[allow(dead_code)]
pub(crate) const UI_COMMAND_CAPACITY: usize = 64;

/// Spawns the UI-command forwarder: drains `commands` into `sink` in
/// arrival order until the channel closes (the UI side dropped — the
/// forwarder's own exit, logged). The caller owns the handle's
/// retirement (await the clean exit, or abort once the UI session is
/// over).
pub fn spawn_ui_command_forwarder<T: CommandSink>(
    mut commands: mpsc::Receiver<StateCommand>,
    sink: T,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            sink.dispatch(command);
        }
        tracing::info!("ui command channel closed; forwarder exiting");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Fake sink: collects dispatch order/content hermetically (fakes
    /// over mocks; no engine spawned). The log lives behind an `Arc` so
    /// the test keeps a handle while the forwarder owns the sink.
    #[derive(Clone, Default)]
    struct FakeSink(Arc<Mutex<Vec<StateCommand>>>);

    impl CommandSink for FakeSink {
        fn dispatch(&self, command: StateCommand) {
            self.0.lock().expect("sink lock").push(command);
        }
    }

    fn sample_commands() -> Vec<StateCommand> {
        use taskboard_domain::StackId;
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        vec![
            StateCommand::CreateTask {
                title: "first".into(),
                stack,
                order: 0,
            },
            StateCommand::CreateTask {
                title: "second".into(),
                stack,
                order: 1,
            },
            StateCommand::CreateStack {
                title: "done".into(),
                order: 2,
            },
        ]
    }

    #[tokio::test]
    async fn forwards_the_sequence_in_order_and_exits_on_close() {
        let sink = FakeSink::default();
        let commands = sample_commands();
        let (tx, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());

        for command in &commands {
            tx.send(command.clone())
                .await
                .expect("channel has capacity");
        }
        drop(tx); // the UI side going away ends the forwarder

        forwarder.await.expect("forwarder joins cleanly");
        let collected = sink.0.lock().expect("sink lock").clone();
        assert_eq!(collected, commands, "exact sequence, order preserved");
    }

    #[tokio::test]
    async fn exits_immediately_when_the_channel_starts_closed() {
        let sink = FakeSink::default();
        // The sender binding is `_`, so the channel starts closed; the
        // receiver is passed to the forwarder as-is.
        let (_, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());
        // Join-based terminal-state assert: no sleeps, no polling.
        forwarder
            .await
            .expect("forwarder exits on a closed channel");
        assert!(sink.0.lock().expect("sink lock").is_empty());
    }
}

//! The network (tokio runtime) thread implementation

use std::{thread::JoinHandle, time::Instant};

use gs_schemas::GameSide;
use thiserror::Error;
use tokio::task::LocalSet;

use crate::prelude::*;

/// A wrapper for a tokio runtime, allowing for easy scheduling of tasks to run within the context of the network thread.
/// [`State`] will be accessible from the network thread commands.
pub struct NetworkThread<State: NetworkThreadState> {
    side: GameSide,
    tokio_thread: JoinHandle<()>,
    channel: AsyncUnboundedSender<NetworkThreadCommand<State>>,
    startup_time: Instant,
}

/// Trait that needs to be implemented for the state object of the network thread.
pub trait NetworkThreadState: 'static {
    /// Command type passed to [`on_command`].
    type StateCommand: Sized + Send + 'static;

    /// Handle a single custom command for the thread.
    async fn on_command(&mut self, command: Self::StateCommand);
    /// Performs a clean shutdown of the network subsystem.
    async fn shutdown(&mut self);
}

enum NetworkThreadCommand<State: NetworkThreadState> {
    Shutdown(AsyncOneshotSender<()>),
    StateCommand(State::StateCommand),
}

/// Potential errors returned when scheduling a function to run on the network thread
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Error)]
pub enum NetworkThreadCommandError {
    /// Happens when the network thread has already been shut down, or has suffered an irrecoverable error.
    #[error("{0:?} network thread has already terminated")]
    NetworkThreadTerminated(GameSide),
}

impl<State: NetworkThreadState> NetworkThread<State> {
    /// Creates a new network thread and tokio runtime for the given game side.
    pub fn new(
        side: GameSide,
        state_factory: impl (AsyncFnOnce(Instant) -> Result<State>) + Send + 'static,
    ) -> Result<Self> {
        let (net_tx, net_rx) = async_unbounded_channel();
        let network_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name(format!("GS {side:?} Network Worker"))
            .build()
            .context("Could not initialize the tokio runtime for the engine")?;

        let startup_time = Instant::now();

        let (result_tx, result_rx) = async_oneshot_channel();
        let result_aware_state_factory = async move || match state_factory(startup_time).await {
            Ok(factory) => {
                result_tx.send(Ok(()));
                Ok(factory)
            }
            Err(e) => {
                result_tx.send(Err(e));
                Err(())
            }
        };

        let tokio_thread = std::thread::Builder::new()
            .name(format!("GS {side:?} Network Thread"))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || Self::thread_main(network_rt, net_rx, result_aware_state_factory, side))
            .context("Could not create a thread for the engine network subsystem")?;

        result_rx.blocking_recv()??;

        Ok(Self {
            side,
            tokio_thread,
            channel: net_tx,
            startup_time,
        })
    }

    /// Gets the side this thread was created for.
    pub fn side(&self) -> GameSide {
        self.side
    }

    /// Returns if the network thread is still alive and accepting commands.
    pub fn is_alive(&self) -> bool {
        (!self.tokio_thread.is_finished()) && !self.channel.is_closed()
    }

    /// Performs a shutdown of the network thread and waits for it to cleanly exit.
    /// Does nothing if the thread is already shut down.
    pub fn sync_shutdown(&self) {
        let (tx, rx) = async_oneshot_channel();
        // In case of errors (already closed the thread), no-op
        let _ = self.channel.send(NetworkThreadCommand::Shutdown(tx));
        let _ = rx.blocking_recv();
    }

    pub fn send_command(&self, command: State::StateCommand) {
        let _ = self.channel.send(NetworkThreadCommand::StateCommand(command));
    }

    pub fn startup_time(&self) -> Instant {
        self.startup_time
    }

    pub fn packet_timestamp(&self) -> u64 {
        self.startup_time.elapsed().as_millis() as u64
    }

    fn thread_main(
        network_rt: tokio::runtime::Runtime,
        ctrl_rx: AsyncUnboundedReceiver<NetworkThreadCommand<State>>,
        state: impl AsyncFnOnce() -> Result<State, ()>,
        side: GameSide,
    ) {
        let _span = tracing::info_span!("net-thread", ?side).entered();
        network_rt.block_on(async move {
            let local_set = LocalSet::new();
            local_set.run_until(Self::thread_localset_main(ctrl_rx, state)).await;
        });
    }

    async fn thread_localset_main(
        mut ctrl_rx: AsyncUnboundedReceiver<NetworkThreadCommand<State>>,
        state: impl AsyncFnOnce() -> Result<State, ()>,
    ) {
        let state = state().await;
        let Ok(mut state) = state else { return };
        while let Some(msg) = ctrl_rx.recv().await {
            match msg {
                NetworkThreadCommand::Shutdown(feedback) => {
                    ctrl_rx.close();
                    state.shutdown().await;
                    let _ = feedback.send(());
                    return;
                }
                NetworkThreadCommand::StateCommand(command) => {
                    state.on_command(command).await;
                }
            }
        }
    }
}

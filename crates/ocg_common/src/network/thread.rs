//! The network (tokio runtime) thread implementation

use std::future::Future;
use std::pin::Pin;
use std::thread::JoinHandle;

use futures::FutureExt;
use ocg_schemas::GameSide;
use thiserror::Error;
use tokio::task::LocalSet;

use crate::prelude::*;

/// A wrapper for a tokio runtime, allowing for easy scheduling of tasks to run within the context of the network thread.
/// [`State`] will be accessible from the network thread commands.
pub struct NetworkThread<State> {
    side: GameSide,
    tokio_thread: JoinHandle<()>,
    channel: AsyncUnboundedSender<NetworkThreadCommand<State>>,
}

/// Trait that needs to be implemented for the state object of the network thread.
pub trait NetworkThreadState: 'static {
    /// Performs a clean shutdown of the network subsystem.
    fn shutdown(this: Rc<RefCell<Self>>) -> impl Future<Output = ()>;
}

type NetworkThreadFunction<State> = dyn FnOnce(&Rc<RefCell<State>>) + Send + 'static;

type NetworkThreadAsyncFuture<'state, Output = ()> = Pin<Box<dyn Future<Output = Output> + 'state>>;
type NetworkThreadAsyncFunction<State> =
    dyn for<'state> FnOnce(&'state Rc<RefCell<State>>) -> NetworkThreadAsyncFuture<'state> + Send + 'static;

enum NetworkThreadCommand<State> {
    Shutdown(AsyncOneshotSender<()>),
    RunInLocalSet(Box<NetworkThreadFunction<State>>),
    RunAsyncInLocalSet(Box<NetworkThreadAsyncFunction<State>>),
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
    pub fn new(side: GameSide, state: impl (FnOnce() -> State) + Send + 'static) -> Self {
        let (net_tx, net_rx) = async_unbounded_channel();
        let network_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name(format!("OCG {side:?} Network Worker"))
            .build()
            .expect("Could not initialize the tokio runtime for the engine");
        let tokio_thread = std::thread::Builder::new()
            .name(format!("OCG {side:?} Network Thread"))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || Self::thread_main(network_rt, net_rx, state))
            .expect("Could not create a thread for the engine");

        Self {
            side,
            tokio_thread,
            channel: net_tx,
        }
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

    /// Runs the given function in the context of the network thread.
    pub fn exec<F: FnOnce(&Rc<RefCell<State>>) + Send + 'static>(
        &self,
        function: F,
    ) -> Result<(), NetworkThreadCommandError> {
        self.exec_boxed(Box::new(function))
    }

    /// Awaits the given future in the network thread.
    pub fn exec_async<
        F: (for<'state> FnOnce(&'state Rc<RefCell<State>>) -> NetworkThreadAsyncFuture<'state>) + Send + 'static,
    >(
        &self,
        function: F,
    ) -> Result<(), NetworkThreadCommandError> {
        self.exec_async_boxed(Box::new(move |state| function(state)))
    }

    /// Awaits the given future in the network thread, then awaits and returns the result of execution on the current thread.
    pub fn exec_async_await<
        Output: Send + 'static,
        F: (for<'state> FnOnce(&'state Rc<RefCell<State>>) -> NetworkThreadAsyncFuture<'state, Output>) + Send + 'static,
    >(
        &self,
        function: F,
    ) -> Result<Output, NetworkThreadCommandError> {
        let (tx, rx) = async_oneshot_channel();
        self.exec_async_boxed(Box::new(move |state| {
            Box::pin(function(state).then(move |out| async move {
                let _ = tx.send(out);
            }))
        }))?;
        rx.blocking_recv()
            .or(Err(NetworkThreadCommandError::NetworkThreadTerminated(self.side)))
    }

    /// Non-generic implementation of exec()
    pub fn exec_boxed(&self, function: Box<NetworkThreadFunction<State>>) -> Result<(), NetworkThreadCommandError> {
        self.channel
            .send(NetworkThreadCommand::RunInLocalSet(function))
            .or(Err(NetworkThreadCommandError::NetworkThreadTerminated(self.side)))
    }

    /// Non-generic implementation of exec()
    pub fn exec_async_boxed(
        &self,
        function: Box<NetworkThreadAsyncFunction<State>>,
    ) -> Result<(), NetworkThreadCommandError> {
        self.channel
            .send(NetworkThreadCommand::RunAsyncInLocalSet(function))
            .or(Err(NetworkThreadCommandError::NetworkThreadTerminated(self.side)))
    }

    fn thread_main(
        network_rt: tokio::runtime::Runtime,
        ctrl_rx: AsyncUnboundedReceiver<NetworkThreadCommand<State>>,
        state: impl FnOnce() -> State,
    ) {
        network_rt.block_on(async move {
            let local_set = LocalSet::new();
            local_set.run_until(Self::thread_localset_main(ctrl_rx, state)).await;
        });
    }

    async fn thread_localset_main(
        mut ctrl_rx: AsyncUnboundedReceiver<NetworkThreadCommand<State>>,
        state: impl FnOnce() -> State,
    ) {
        let state = Rc::new(RefCell::new(state()));
        while let Some(msg) = ctrl_rx.recv().await {
            match msg {
                NetworkThreadCommand::Shutdown(feedback) => {
                    ctrl_rx.close();
                    State::shutdown(state).await;
                    let _ = feedback.send(());
                    return;
                }
                NetworkThreadCommand::RunInLocalSet(lambda) => {
                    lambda(&state);
                }
                NetworkThreadCommand::RunAsyncInLocalSet(lambda) => {
                    let future = lambda(&state);
                    future.await;
                }
            }
        }
    }
}

//! The network (tokio runtime) thread implementation

use std::future::Future;
use std::pin::Pin;
use std::thread::JoinHandle;

use futures::FutureExt;
use gs_schemas::schemas::NetworkStreamHeader;
use gs_schemas::GameSide;
use hashbrown::HashMap;
use thiserror::Error;
use tokio::task::LocalSet;

use super::transport::InProcessStream;
use crate::prelude::*;

/// A wrapper for a tokio runtime, allowing for easy scheduling of tasks to run within the context of the network thread.
/// [`State`] will be accessible from the network thread commands.
pub struct NetworkThread<State> {
    side: GameSide,
    tokio_thread: JoinHandle<()>,
    channel: AsyncUnboundedSender<NetworkThreadCommand<State>>,
    new_stream_handler: Mutex<HashMap<NetworkStreamHeader, Box<NetworkThreadStreamHandler<State>>>>,
}

/// Trait that needs to be implemented for the state object of the network thread.
pub trait NetworkThreadState: 'static {
    /// Performs a clean shutdown of the network subsystem.
    fn shutdown(this: Rc<RefCell<Self>>) -> impl Future<Output = ()>;
}

/// A boxed future that can be queued on the network thread's tokio LocalSet.
pub type NetworkThreadAsyncFuture<'state, Output = ()> = Pin<Box<dyn Future<Output = Output> + 'state>>;
/// A future factory function used for network thread tasks.
pub type NetworkThreadAsyncFunction<State> =
    dyn for<'state> FnOnce(&'state Rc<RefCell<State>>) -> NetworkThreadAsyncFuture<'state> + Send + 'static;
/// Handler for newly opened async streams.
pub type NetworkThreadStreamHandler<State> =
    dyn FnMut(Rc<RefCell<State>>, InProcessStream) -> NetworkThreadAsyncFuture<'static> + Send + 'static;

enum NetworkThreadCommand<State> {
    Shutdown(AsyncOneshotSender<()>),
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
            .thread_name(format!("GS {side:?} Network Worker"))
            .build()
            .expect("Could not initialize the tokio runtime for the engine");
        let tokio_thread = std::thread::Builder::new()
            .name(format!("GS {side:?} Network Thread"))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || Self::thread_main(network_rt, net_rx, state, side))
            .expect("Could not create a thread for the engine");

        Self {
            side,
            tokio_thread,
            channel: net_tx,
            new_stream_handler: Mutex::new(HashMap::with_capacity(32)),
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

    /// Schedules a future in the network thread, the future is made using the provided factory function.
    pub fn schedule_task<
        F: (for<'state> FnOnce(&'state Rc<RefCell<State>>) -> NetworkThreadAsyncFuture<'state, Result<Output>>)
            + Send
            + 'static,
        Output: Send + 'static,
    >(
        &self,
        function: F,
    ) -> AsyncResult<Output> {
        let (result, tx) = AsyncResult::new_pair();
        let queue_result = self.schedule_task_boxed(Box::new(move |state| {
            Box::pin(function(state).then(|out| async move { drop(tx.send(out)) }))
        }));
        if let Err(e) = queue_result {
            return AsyncResult::new_err(e.into());
        }
        result
    }

    /// Non-generic implementation of exec()
    pub fn schedule_task_boxed(
        &self,
        function: Box<NetworkThreadAsyncFunction<State>>,
    ) -> Result<(), NetworkThreadCommandError> {
        self.channel
            .send(NetworkThreadCommand::RunAsyncInLocalSet(function))
            .or(Err(NetworkThreadCommandError::NetworkThreadTerminated(self.side)))
    }

    /// Registers a new stream type handler for the given header, overwrites any previous handler with the same header.
    pub fn insert_stream_handler(&self, header: NetworkStreamHeader, function: Box<NetworkThreadStreamHandler<State>>) {
        let mut map = self.new_stream_handler.lock().unwrap();
        map.insert(header, function);
    }

    /// Creates a stream handler future by looking up the matching factory function, or returns Err if none were registered for this header.
    pub fn create_stream_handler(
        &self,
        state: Rc<RefCell<State>>,
        stream: InProcessStream,
    ) -> Result<NetworkThreadAsyncFuture<'static>, InProcessStream> {
        let mut factory = self.new_stream_handler.lock().unwrap();
        let factory = factory.get_mut(&stream.header);
        match factory {
            Some(factory) => Ok(factory(state, stream)),
            None => Err(stream),
        }
    }

    fn thread_main(
        network_rt: tokio::runtime::Runtime,
        ctrl_rx: AsyncUnboundedReceiver<NetworkThreadCommand<State>>,
        state: impl FnOnce() -> State,
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
                NetworkThreadCommand::RunAsyncInLocalSet(lambda) => {
                    let future = lambda(&state);
                    future.await;
                }
            }
        }
    }
}

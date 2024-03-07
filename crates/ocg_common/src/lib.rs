#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]

//! The common client&server code for OpenCubeGame

pub mod network;
pub mod prelude;
pub mod voxel;

use std::thread::JoinHandle;
use std::time::Duration;

use bevy::app::AppExit;
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::time::TimePlugin;
use bevy::utils::synccell::SyncCell;
use ocg_schemas::voxel::voxeltypes::BlockRegistry;
use ocg_schemas::OcgExtraData;
use tokio::io::{duplex, DuplexStream};
use tokio::task::LocalSet;

use crate::network::transport::create_local_rpc_server;
use crate::network::PeerAddress;
use crate::prelude::*;

/// Target (maximum) number of game simulation ticks in a second.
pub const TICKS_PER_SECOND: i32 = 32;
/// Target (maximum) number of game simulation ticks in a second, as a `f32`.
pub const TICKS_PER_SECOND_F32: f32 = TICKS_PER_SECOND as f32;
/// Target (maximum) number of game simulation ticks in a second, as a `f64`.
pub const TICKS_PER_SECOND_F64: f64 = TICKS_PER_SECOND as f64;
/// Target (minimum) number of seconds in a game simulation tick, as a `f32`.
pub const SECONDS_PER_TICK_F32: f32 = 1.0f32 / TICKS_PER_SECOND as f32;
/// Target (minimum) number of seconds in a game simulation tick, as a `f64`.
pub const SECONDS_PER_TICK_F64: f64 = 1.0f64 / TICKS_PER_SECOND as f64;
/// Target (minimum) number of microseconds in a game simulation tick, as a `i64`.
pub const MICROSECONDS_PER_TICK: i64 = 1_000_000i64 / TICKS_PER_SECOND as i64;
/// One game tick as a [`Duration`]
pub const TICK: Duration = Duration::from_micros(MICROSECONDS_PER_TICK as u64);

/// Size in bytes of the internal client-server "socket" buffer.
const INPROCESS_SOCKET_BUFFER_SIZE: usize = 1024 * 1024;

// Ensure `MICROSECONDS_PER_TICK` is perfectly accurate.
static_assertions::const_assert_eq!(1_000_000i64 / MICROSECONDS_PER_TICK, TICKS_PER_SECOND as i64);

/// An [`OcgExtraData`] implementation containing server-side data for the game engine.
/// The struct holds server state, the trait points to per chunk/group/etc. data.
#[derive(Clone)]
pub struct ServerData {
    /// A full registry of block types currently in game.
    pub block_registry: Arc<BlockRegistry>,
}

impl OcgExtraData for ServerData {
    type ChunkData = ();
    type GroupData = ();
}

/// Control commands for the server, for in-process communication.
pub enum GameServerControlCommand {
    /// Gracefully shuts down the server, notifies on the given channel when done.
    Shutdown(AsyncOneshotSender<()>),
    /// Creates a new local (in-process) player connection, returns the result asynchronously on the given channel.
    CreateLocalConnection(AsyncOneshotSender<(PeerAddress, DuplexStream)>),
}

type NetworkThreadLambda = dyn FnOnce(&Arc<GameServer>) + Send + 'static;

enum NetworkThreadCommand {
    Shutdown(AsyncOneshotSender<()>),
    /// Runs the given function in the context of a LocalSet on the network thread.
    Run(Box<NetworkThreadLambda>),
}

/// A struct to communicate with the "server"-side engine that runs the game simulation.
/// It has its own bevy App with a very limited set of plugins enabled to be able to run without a graphical user interface.
pub struct GameServer {
    engine_thread: JoinHandle<()>,
    network_thread: JoinHandle<()>,
    pause: AtomicBool,
    network_rt: tokio::runtime::Runtime,
}

/// A handle to a [`GameServer`] and its in-process control channel.
pub struct GameServerHandle {
    /// The spawned [`GameServer`] instance.
    pub server: Arc<GameServer>,
    /// The channel for sending [`GameServerControlCommand`] such as "Shutdown".
    pub control_channel: StdUnboundedSender<GameServerControlCommand>,
    network_channel: AsyncUnboundedSender<NetworkThreadCommand>,
}

/// A handle to a [`GameServer`] accessible from within bevy systems.
#[derive(Resource, Clone)]
pub struct GameServerResource(Arc<GameServer>);

#[derive(Resource)]
struct GameServerControlCommandReceiver(SyncCell<StdUnboundedReceiver<GameServerControlCommand>>);

#[derive(Resource)]
struct NetworkServerControlCommandSender(SyncCell<AsyncUnboundedSender<NetworkThreadCommand>>);

impl GameServer {
    /// Spawns a new thread that runs the engine in a paused state, and returns a handle to control it.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Result<GameServerHandle> {
        let (tx, rx) = std_bounded_channel(1);
        let (ntx, nrx) = async_oneshot_channel();
        let (ctrl_tx, ctrl_rx) = std_unbounded_channel();
        let (net_tx, net_rx) = async_unbounded_channel();
        let network_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("OCG Server")
            .build()?;
        let net_tx2 = net_tx.clone();
        let engine_thread = std::thread::Builder::new()
            .name("OCG Server Engine Thread".to_owned())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || GameServer::engine_thread_main(rx, ctrl_rx, net_tx2))
            .expect("Could not create a thread for the engine");
        let network_thread = std::thread::Builder::new()
            .name("OCG Server Network Thread".to_owned())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || GameServer::network_thread_main(nrx, net_rx))
            .expect("Could not create a thread for the engine");
        let server = Self {
            engine_thread,
            network_thread,
            pause: AtomicBool::new(true),
            network_rt,
        };
        let server = Arc::new(server);
        tx.send(Arc::clone(&server))
            .expect("Could not pass initialization data to the server engine thread");
        ntx.send(Arc::clone(&server))
            .ok()
            .expect("Could not pass initialization data to the server network thread.");
        Ok(GameServerHandle {
            server,
            control_channel: ctrl_tx,
            network_channel: net_tx,
        })
    }

    /// Constructs a simple server for unit tests with no disk IO/savefile location attached.
    pub fn new_test() -> GameServerHandle {
        Self::new().expect("Could not create a GameServer test instance")
    }

    /// Checks if the game logic is paused.
    pub fn is_paused(&self) -> bool {
        self.pause.load(AtomicOrdering::SeqCst)
    }

    /// Sets the paused state for game logic, returns the previous state.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        self.pause.swap(paused, AtomicOrdering::SeqCst)
    }

    /// Checks if the engine thread is still alive.
    pub fn is_alive(&self) -> bool {
        !self.engine_thread.is_finished()
    }

    /// Checks if the network thread is still alive.
    pub fn is_network_alive(&self) -> bool {
        !self.network_thread.is_finished()
    }

    fn network_thread_main(
        nrx: AsyncOneshotReceiver<Arc<GameServer>>,
        mut ctrl_rx: AsyncUnboundedReceiver<NetworkThreadCommand>,
    ) {
        let engine = nrx.blocking_recv().expect("Engine not sent");
        let netrt_engine = Arc::clone(&engine);

        netrt_engine.network_rt.block_on(async move {
            let local_set = LocalSet::new();
            local_set
                .run_until(async move {
                    while let Some(msg) = ctrl_rx.recv().await {
                        if !Self::network_thread_on_msg(&engine, msg).await {
                            break;
                        }
                    }
                })
                .await;
        });
    }

    async fn network_thread_on_msg(engine: &Arc<GameServer>, msg: NetworkThreadCommand) -> bool {
        match msg {
            NetworkThreadCommand::Shutdown(feedback) => {
                let _ = feedback.send(());
                return false;
            }
            NetworkThreadCommand::Run(lambda) => {
                lambda(engine);
            }
        }
        true
    }

    fn engine_thread_main(
        engine: StdUnboundedReceiver<Arc<GameServer>>,
        ctrl_rx: StdUnboundedReceiver<GameServerControlCommand>,
        net_tx: tokio::sync::mpsc::UnboundedSender<NetworkThreadCommand>,
    ) {
        let engine = {
            let e = engine
                .recv()
                .expect("Could not receive initialization data in the engine thread");
            drop(engine); // force-drop the receiver early to not hold onto its memory
            e
        };
        let mut app = App::new();
        app.add_plugins(LogPlugin::default())
            .add_plugins(TaskPoolPlugin::default())
            .add_plugins(TypeRegistrationPlugin)
            .add_plugins(FrameCountPlugin)
            .add_plugins(TimePlugin)
            .add_plugins(TransformPlugin)
            .add_plugins(HierarchyPlugin)
            .add_plugins(DiagnosticsPlugin)
            .add_plugins(AssetPlugin::default())
            .add_plugins(AnimationPlugin);
        app.insert_resource(GameServerResource(engine));
        app.insert_resource(Time::<Fixed>::from_duration(TICK));
        app.insert_resource(GameServerControlCommandReceiver(SyncCell::new(ctrl_rx)));
        app.insert_resource(NetworkServerControlCommandSender(SyncCell::new(net_tx)));
        app.add_systems(PostUpdate, Self::control_command_handler_system);
        app.run();
    }

    fn control_command_handler_system(
        engine: Res<GameServerResource>,
        ctrl_rx: ResMut<GameServerControlCommandReceiver>,
        net_tx: ResMut<NetworkServerControlCommandSender>,
        mut exiter: EventWriter<AppExit>,
    ) {
        let ctrl_rx = ctrl_rx.into_inner().0.get();
        let net_tx = net_tx.into_inner().0.get();
        let _engine = &engine.into_inner().0;
        for cmd in ctrl_rx.try_iter() {
            match cmd {
                GameServerControlCommand::Shutdown(notif) => {
                    let (feedback_tx, feedback_rx) = async_oneshot_channel();
                    let _ = net_tx.send(NetworkThreadCommand::Shutdown(feedback_tx));
                    let _ = feedback_rx.blocking_recv();
                    exiter.send(AppExit);
                    let _ = notif.send(());
                }
                GameServerControlCommand::CreateLocalConnection(rstx) => {
                    net_tx
                        .send(NetworkThreadCommand::Run(Box::new(move |engine| {
                            let addr = PeerAddress::Local(0);
                            let (spipe, cpipe) = duplex(INPROCESS_SOCKET_BUFFER_SIZE);
                            let rpc_server = create_local_rpc_server(Arc::clone(engine), spipe, addr);
                            let _s_disconnector = rpc_server.get_disconnector();
                            tokio::task::spawn_local(async move {
                                rstx.send((addr, cpipe)).ok().context(
                                    "Could not send GameServerControlCommand::CreateLocalConnection response",
                                )?;
                                rpc_server.await.context("Local RPC server failure")
                            });
                        })))
                        .unwrap();
                }
            }
        }
    }
}

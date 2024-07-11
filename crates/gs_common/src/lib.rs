#![warn(missing_docs)]
#![deny(
    clippy::disallowed_types,
    clippy::await_holding_refcell_ref,
    clippy::await_holding_lock
)]
#![allow(clippy::type_complexity)]

//! The common client&server code for Geosia

pub mod config;
pub mod network;
pub mod prelude;
pub mod promises;
pub mod voxel;

use std::thread::JoinHandle;
use std::time::Duration;

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use bevy::utils::synccell::SyncCell;
use gs_schemas::registries::GameRegistries;
use gs_schemas::registry::Registry;
use gs_schemas::{GameSide, GsExtraData};
use smallvec::SmallVec;
use voxel::persistence::generator::GeneratorPersistenceLayer;
use voxel::plugin::VoxelUniverseBuilder;

use crate::config::{GameConfig, GameConfigHandle};
use crate::network::server::{LocalConnectionPipe, NetworkServerPlugin, NetworkThreadServerState};
use crate::network::thread::NetworkThread;
use crate::prelude::*;
use crate::voxel::generator::multi_noise::MultiNoiseGenerator;
use crate::voxel::persistence::memory::MemoryPersistenceLayer;
use crate::voxel::plugin::VoxelUniversePlugin;

// TODO: Populate these from build/git info
/// The major SemVer field of the current build's version
pub static GAME_VERSION_MAJOR: u32 = 0;
/// The minor SemVer field of the current build's version
pub static GAME_VERSION_MINOR: u32 = 0;
/// The patch SemVer field of the current build's version
pub static GAME_VERSION_PATCH: u32 = 1;
/// The build SemVer field of the current build's version
pub static GAME_VERSION_BUILD: &str = "todo";
/// The prerelease SemVer field of the current build's version
pub static GAME_VERSION_PRERELEASE: &str = "";
/// The name of the game
pub static GAME_BRAND_NAME: &str = "Geosia";

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

// Ensure `MICROSECONDS_PER_TICK` is perfectly accurate.
static_assertions::const_assert_eq!(1_000_000i64 / MICROSECONDS_PER_TICK, TICKS_PER_SECOND as i64);

/// The tag for systems that should run while in game.
#[derive(SystemSet, Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InGameSystemSet;

/// An [`GsExtraData`] implementation containing server-side data for the game engine.
/// The struct holds server state, the trait points to per chunk/group/etc. data.
pub struct ServerData {
    /// Shared client/server registries.
    pub shared_registries: GameRegistries,
}

impl GsExtraData for ServerData {
    type ChunkData = voxel::plugin::ServerChunkMetadata;
    type GroupData = ();

    const SIDE: GameSide = GameSide::Server;
}

/// A command that can be remotely executed on the bevy world.
pub type GameBevyCommand<Output = ()> = dyn (FnOnce(&mut World) -> Output) + Send + 'static;

/// Control commands for the server, for in-process communication.
pub enum GameServerControlCommand {
    /// Gracefully shuts down the server, notifies on the given channel when done.
    Shutdown(AsyncOneshotSender<()>),
    /// Queues the given command to run in an exclusive system with full World access.
    Invoke(Box<GameBevyCommand>),
}

/// A struct to communicate with the "server"-side engine that runs the game simulation.
/// It has its own bevy App with a very limited set of plugins enabled to be able to run without a graphical user interface.
pub struct GameServer {
    config: GameConfigHandle,
    server_data: ServerData,
    engine_thread: JoinHandle<()>,
    network_thread: NetworkThread<NetworkThreadServerState>,
    pause: AtomicBool,
    control_channel: StdUnboundedSender<GameServerControlCommand>,
}

/// A handle to a [`GameServer`] accessible from within bevy systems.
#[derive(Resource, Clone)]
pub struct GameServerResource(Arc<GameServer>);

#[derive(Resource)]
struct GameServerControlCommandReceiver(SyncCell<StdUnboundedReceiver<GameServerControlCommand>>);

impl GameServer {
    /// Spawns a new thread that runs the engine in a paused state, and returns a handle to control it.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(config: GameConfigHandle) -> Result<Arc<GameServer>> {
        let (tx, rx) = std_bounded_channel(1);
        let (ctrl_tx, ctrl_rx) = std_unbounded_channel();

        let network_thread = NetworkThread::new(GameSide::Server, NetworkThreadServerState::new);

        let engine_thread = std::thread::Builder::new()
            .name("GS Server Engine Thread".to_owned())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || GameServer::engine_thread_main(rx, ctrl_rx))
            .expect("Could not create a thread for the engine");

        let server_data = ServerData {
            shared_registries: builtin_game_registries(),
        };

        let server = Self {
            config,
            server_data,
            engine_thread,
            network_thread,
            pause: AtomicBool::new(true),
            control_channel: ctrl_tx,
        };
        let server = Arc::new(server);
        tx.send(Arc::clone(&server))
            .expect("Could not pass initialization data to the server engine thread");
        Ok(server)
    }

    /// Constructs a simple server for unit tests with no disk IO/savefile location attached.
    pub fn new_test() -> Arc<GameServer> {
        let mut game_config = GameConfig::default();
        "Test server".clone_into(&mut game_config.server.server_title);
        game_config.server.server_subtitle = format!("Thread {:?}", std::thread::current().id());
        game_config.server.listen_addresses.clear();
        Self::new(GameConfig::new_handle(game_config)).expect("Could not create a GameServer test instance")
    }

    /// Returns a shared accessor to the global game configuration handle.
    pub fn config(&self) -> &AsyncWatchReceiver<GameConfig> {
        &self.config.1
    }

    /// Returns a shared publisher to the global game configuration handle.
    pub fn config_updater(&self) -> &AsyncWatchSender<GameConfig> {
        &self.config.0
    }

    /// Returns the game configuration handle.
    pub fn config_handle(&self) -> &GameConfigHandle {
        &self.config
    }

    /// Checks if the game logic is paused.
    pub fn is_paused(&self) -> bool {
        self.pause.load(AtomicOrdering::SeqCst)
    }

    /// Sets the paused state for game logic, returns the previous state.
    pub fn set_paused(&self, paused: bool) -> bool {
        self.pause.swap(paused, AtomicOrdering::SeqCst)
    }

    /// Checks if the engine thread is still alive.
    pub fn is_alive(&self) -> bool {
        !self.engine_thread.is_finished()
    }

    /// Checks if the network thread is still alive.
    pub fn is_network_alive(&self) -> bool {
        self.network_thread.is_alive()
    }

    /// Queues the given function to run with exclusive access to the bevy [`World`].
    pub fn schedule_bevy<
        BevyCmd: (FnOnce(&mut World) -> Result<Output>) + Send + 'static,
        Output: Send + Sync + 'static,
    >(
        &self,
        cmd: BevyCmd,
    ) -> AsyncResult<Output> {
        let (result, tx) = AsyncResult::new_pair();
        self.schedule_bevy_boxed(Box::new(move |world| drop(tx.send(cmd(world)))));
        result
    }

    /// Non-generic version of [`Self::schedule_bevy`]
    pub fn schedule_bevy_boxed(&self, cmd: Box<GameBevyCommand>) {
        let _ = self.control_channel.send(GameServerControlCommand::Invoke(cmd));
    }

    /// Asynchronously creates a new local connection to this server's network runtime.
    pub fn create_local_connection(self: &Arc<Self>) -> AsyncResult<LocalConnectionPipe> {
        let inner_engine = Arc::clone(self);
        self.network_thread.schedule_task(move |state| {
            Box::pin(NetworkThreadServerState::accept_local_connection(state, inner_engine))
        })
    }

    fn engine_thread_main(
        engine: StdUnboundedReceiver<Arc<GameServer>>,
        ctrl_rx: StdUnboundedReceiver<GameServerControlCommand>,
    ) {
        let engine = {
            let e = engine
                .recv()
                .expect("Could not receive initialization data in the engine thread");
            drop(engine); // force-drop the receiver early to not hold onto its memory
            e
        };
        let mut app = App::new();
        app.add_plugins(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions {
                compute: bevy::core::TaskPoolThreadAssignmentPolicy {
                    min_threads: 1,
                    max_threads: 10,
                    percent: 0.75,
                },
                ..default()
            },
        });
        app.add_plugins(TypeRegistrationPlugin)
            .add_plugins(StatesPlugin)
            .add_plugins(FrameCountPlugin)
            .add_plugins(TimePlugin)
            .add_plugins(TransformPlugin)
            .add_plugins(HierarchyPlugin)
            .add_plugins(DiagnosticsPlugin)
            .add_plugins(AssetPlugin::default())
            .add_plugins(AnimationPlugin)
            .add_plugins(ScheduleRunnerPlugin::run_loop(TICK));

        app.add_plugins(VoxelUniversePlugin::<ServerData>::new())
            .add_plugins(NetworkServerPlugin);

        let block_registry = Arc::clone(&engine.server_data.shared_registries.block_types);
        let biome_registry = Arc::clone(&engine.server_data.shared_registries.biome_types);

        let generator = MultiNoiseGenerator::new(123456789, Arc::clone(&biome_registry), Arc::clone(&block_registry));
        let gen_world = GeneratorPersistenceLayer::new(Arc::new(generator), default());
        let persistence = MemoryPersistenceLayer::new(Box::new(gen_world));

        fn configure_sets(app: &mut App, schedule: impl ScheduleLabel) {
            app.configure_sets(schedule, InGameSystemSet);
        }
        configure_sets(&mut app, PreUpdate);
        configure_sets(&mut app, Update);
        configure_sets(&mut app, PostUpdate);
        configure_sets(&mut app, FixedPreUpdate);
        configure_sets(&mut app, FixedUpdate);
        configure_sets(&mut app, FixedPostUpdate);

        app.insert_resource(Time::<Fixed>::from_duration(TICK));
        app.insert_resource(GameServerControlCommandReceiver(SyncCell::new(ctrl_rx)));
        app.insert_resource(GameServerResource(engine));

        VoxelUniverseBuilder::<ServerData>::new(app.world_mut(), block_registry, biome_registry)
            .unwrap()
            .with_persistent_storage(Box::new(persistence))
            .unwrap()
            .build();

        app.add_systems(Startup, Self::network_startup_system);
        app.add_systems(FixedPostUpdate, Self::control_command_handler_system);
        info!("Engine thread starting");
        app.run();
        info!("Engine thread terminating");
    }

    fn network_startup_system(engine: Res<GameServerResource>) {
        let engine = &engine.into_inner().0;
        let net_engine = Arc::clone(engine);
        info!("Bootstrapping network");
        engine
            .network_thread
            .schedule_task(move |state| {
                Box::pin(async move {
                    NetworkThreadServerState::bootstrap(state, net_engine).await?;
                    NetworkThreadServerState::allow_streams(state).await;
                    Ok(())
                })
            })
            .blocking_wait()
            .unwrap();
        info!("Bootstrapping network done");
    }

    fn control_command_handler_system(world: &mut World) {
        let pending_cmds: SmallVec<[GameServerControlCommand; 32]> = {
            let mut ctrl_rx: Mut<GameServerControlCommandReceiver> = world.resource_mut();
            SmallVec::from_iter(ctrl_rx.as_mut().0.get().try_iter())
        };
        for cmd in pending_cmds {
            match cmd {
                GameServerControlCommand::Shutdown(notif) => {
                    info!("Engine thread shutdown command received");
                    let engine: &GameServerResource = world.resource();
                    let engine = &engine.0;
                    engine.network_thread.sync_shutdown();
                    world.send_event(AppExit::Success);
                    let _ = notif.send(());
                }
                GameServerControlCommand::Invoke(cmd) => {
                    cmd(world);
                }
            }
        }
    }
}

/// Simple hardcoded registries of some game objects.
pub fn builtin_game_registries() -> GameRegistries {
    let mut block_types = Registry::default();
    voxel::blocks::setup_basic_blocks(&mut block_types);
    let mut biome_types = Registry::default();
    voxel::biomes::setup_basic_biomes(&mut biome_types);

    GameRegistries {
        block_types: Arc::new(block_types),
        biome_types: Arc::new(biome_types),
    }
}

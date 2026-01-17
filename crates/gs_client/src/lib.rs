//! The clientside of Geosia
mod debugcam;
pub mod network;
pub mod prelude;
pub mod states;
pub mod ui;
pub mod voxel;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::log::LogPlugin;
use bevy::platform::cell::SyncCell;
use bevy::window::{CursorOptions, ExitCondition, PresentMode};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use gs_common::network::thread::NetworkThread;
use gs_common::{GAME_BRAND_NAME, GameBevyCommand};
use gs_schemas::dependencies::smallvec::SmallVec;
use gs_schemas::{GameSide, GsExtraData};
use network::client_packet_handlers::ClientPacketHandlerPlugin;
use states::{ClientAppState, InGameSystemSet, LoadingGameSystemSet, MainMenuSystemSet};
use voxel::ClientVoxelUniversePlugin;

use crate::network::NetworkThreadClientState;
use crate::prelude::*;
use crate::voxel::client_plugin::VoxelUniverseClientPlugin;

/// An [`GsExtraData`] implementation containing the client-side data for the game engine.
#[derive(Copy, Clone, Default, Debug)]
pub struct ClientData;

impl GsExtraData for ClientData {
    type ChunkData = voxel::ClientChunkData;
    type GroupData = voxel::ClientChunkGroupData;

    const SIDE: GameSide = GameSide::Client;
}

/// Channel for executing commands on the client bevy App.
pub type GameControlChannel = StdUnboundedSender<Box<GameBevyCommand>>;

/// The entry point to the client executable
pub fn client_main() {
    // Safety: no other threads should be running at this point.
    unsafe {
        // Unset the manifest dir to make bevy load assets from the workspace root
        std::env::set_var("CARGO_MANIFEST_DIR", "");
    }

    let mut app = App::new();
    // Bevy Base
    app.add_plugins(
        DefaultPlugins
            .set(TaskPoolPlugin {
                task_pool_options: TaskPoolOptions {
                    compute: bevy::app::TaskPoolThreadAssignmentPolicy {
                        min_threads: 1,
                        max_threads: 10,
                        percent: 0.5,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    ..default()
                },
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: GAME_BRAND_NAME.to_string(),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }),
                primary_cursor_options: Some(CursorOptions::default()),
                exit_condition: ExitCondition::OnPrimaryClosed,
                close_when_requested: true,
            })
            .build()
            .disable::<LogPlugin>(),
    );
    // Bevy plugins
    app.add_plugins(EguiPlugin::default());

    app.init_state::<ClientAppState>();
    fn configure_sets(app: &mut App, schedule: impl ScheduleLabel) {
        app.configure_sets(
            schedule,
            (
                MainMenuSystemSet.run_if(in_state(ClientAppState::MainMenu)),
                LoadingGameSystemSet.run_if(in_state(ClientAppState::LoadingGame)),
                InGameSystemSet.run_if(in_state(ClientAppState::InGame)),
            ),
        );
    }
    configure_sets(&mut app, EguiPrimaryContextPass);
    configure_sets(&mut app, PreUpdate);
    configure_sets(&mut app, Update);
    configure_sets(&mut app, PostUpdate);
    configure_sets(&mut app, FixedPreUpdate);
    configure_sets(&mut app, FixedUpdate);
    configure_sets(&mut app, FixedPostUpdate);

    app.add_plugins(ui::common_game_ui_plugin)
        .add_plugins(debugcam::PlayerPlugin)
        .add_plugins(VoxelUniverseClientPlugin)
        .add_plugins(states::main_menu::MainMenuPlugin)
        .add_plugins(states::loading_game::LoadingGamePlugin)
        .add_plugins(states::in_game::InGamePlugin)
        .add_plugins(ClientVoxelUniversePlugin)
        .add_plugins(ClientPacketHandlerPlugin)
        .add_plugins(ui::chat::chat_plugin)
        .add_plugins(debug_window::DebugWindow);

    app.add_systems(PostUpdate, control_command_handler_system);

    app.run();
}

#[derive(Resource)]
struct GameClientControlCommandReceiver(SyncCell<StdUnboundedReceiver<Box<GameBevyCommand>>>);

#[derive(Resource)]
struct ClientNetworkThreadHolder(Arc<NetworkThread<NetworkThreadClientState>>);

fn control_command_handler_system(world: &mut World) {
    let pending_cmds: SmallVec<[Box<GameBevyCommand>; 32]> = {
        let Some(mut ctrl_rx) = world.get_resource_mut::<GameClientControlCommandReceiver>() else {
            return;
        };
        SmallVec::from_iter(ctrl_rx.as_mut().0.get().try_iter())
    };
    for cmd in pending_cmds {
        cmd(world);
    }
}

mod debug_window {
    use std::f32::consts::PI;

    use bevy::color::palettes::tailwind;

    use crate::prelude::*;

    pub struct DebugWindow;

    impl Plugin for DebugWindow {
        fn build(&self, app: &mut App) {
            app.add_systems(Startup, debug_window_setup);
        }
    }

    fn debug_window_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
        warn!("Setting up debug window");
        let _ = asset_server.load::<Font>("fonts/cascadiacode.ttf");

        commands.spawn((
            DirectionalLight {
                shadows_enabled: false,
                illuminance: light_consts::lux::AMBIENT_DAYLIGHT * 0.75,
                ..default()
            },
            Transform {
                translation: vec3(0.0, 1000.0, 0.0),
                rotation: Quat::from_rotation_x(PI / 4.0) * Quat::from_rotation_y(PI / 16.0),
                scale: Vec3::ONE,
            },
        ));
        commands.spawn((
            DirectionalLight {
                shadows_enabled: false,
                illuminance: light_consts::lux::AMBIENT_DAYLIGHT * 0.25,
                ..default()
            },
            Transform {
                translation: vec3(0.0, 1000.0, 0.0),
                rotation: Quat::from_rotation_x(PI / 4.0) * Quat::from_rotation_y(PI + PI / 16.0),
                scale: Vec3::ONE,
            },
        ));
        commands.insert_resource(GlobalAmbientLight {
            color: tailwind::GRAY_50.into(),
            brightness: 10.0,
            affects_lightmapped_meshes: true,
        });
        warn!("Setting up debug window done");
    }
}

#![warn(missing_docs)]
#![deny(
    clippy::disallowed_types,
    clippy::await_holding_refcell_ref,
    clippy::await_holding_lock
)]
#![allow(clippy::type_complexity)]

//! The clientside of Geosia
mod debugcam;
pub mod network;
pub mod states;
pub mod voxel;

use bevy::a11y::AccessibilityPlugin;
use bevy::audio::AudioPlugin;
use bevy::core_pipeline::CorePipelinePlugin;
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::gltf::GltfPlugin;
use bevy::input::InputPlugin;
use bevy::pbr::PbrPlugin;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::RenderPlugin;
use bevy::scene::ScenePlugin;
use bevy::sprite::SpritePlugin;
use bevy::state::app::StatesPlugin;
use bevy::text::TextPlugin;
use bevy::time::TimePlugin;
use bevy::ui::UiPlugin;
use bevy::utils::synccell::SyncCell;
use bevy::window::{ExitCondition, PresentMode};
use bevy::winit::WinitPlugin;
use bevy_egui::EguiPlugin;
use gs_common::network::thread::NetworkThread;
use gs_common::prelude::*;
use gs_common::{GameBevyCommand, GAME_BRAND_NAME};
use gs_schemas::dependencies::smallvec::SmallVec;
use gs_schemas::registries::GameRegistries;
use gs_schemas::{GameSide, GsExtraData};
use states::{ClientAppState, InGameSystemSet, LoadingGameSystemSet, MainMenuSystemSet};

use crate::network::NetworkThreadClientState;
use crate::voxel::client_plugin::VoxelUniverseClientPlugin;

/// An [`GsExtraData`] implementation containing the client-side data for the game engine.
#[derive(Resource)]
pub struct ClientData {
    /// Shared client/server registries.
    pub shared_registries: GameRegistries,
}

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
    app.add_plugins(TaskPoolPlugin {
        task_pool_options: TaskPoolOptions {
            compute: bevy::core::TaskPoolThreadAssignmentPolicy {
                min_threads: 1,
                max_threads: 10,
                percent: 0.5,
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
        .add_plugins(InputPlugin)
        .add_plugins(WindowPlugin {
            primary_window: Some(Window {
                title: GAME_BRAND_NAME.to_string(),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            exit_condition: ExitCondition::OnPrimaryClosed,
            close_when_requested: true,
        })
        .add_plugins(AccessibilityPlugin)
        .add_plugins(AssetPlugin::default())
        .add_plugins(ScenePlugin)
        .add_plugins(WinitPlugin::<bevy::winit::WakeUp>::default())
        .add_plugins(RenderPlugin::default())
        .add_plugins(ImagePlugin::default())
        .add_plugins(PipelinedRenderingPlugin)
        .add_plugins(CorePipelinePlugin)
        .add_plugins(SpritePlugin { add_picking: false })
        .add_plugins(TextPlugin)
        .add_plugins(UiPlugin {
            add_picking: false,
            enable_rendering: true,
        })
        .add_plugins(PbrPlugin::default())
        .add_plugins(AudioPlugin::default())
        .add_plugins(GilrsPlugin)
        .add_plugins(AnimationPlugin)
        .add_plugins(GltfPlugin::default());
    // Bevy plugins
    app.add_plugins(EguiPlugin);

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
    configure_sets(&mut app, PreUpdate);
    configure_sets(&mut app, Update);
    configure_sets(&mut app, PostUpdate);
    configure_sets(&mut app, FixedPreUpdate);
    configure_sets(&mut app, FixedUpdate);
    configure_sets(&mut app, FixedPostUpdate);

    app.add_plugins(debugcam::PlayerPlugin)
        .add_plugins(VoxelUniverseClientPlugin)
        .add_plugins(states::main_menu::MainMenuPlugin)
        .add_plugins(states::loading_game::LoadingGamePlugin)
        .add_plugins(states::in_game::InGamePlugin);

    app.add_plugins(debug_window::DebugWindow);
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
    use bevy::prelude::*;

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
                illuminance: 1000.0,
                ..default()
            },
            Transform::from_xyz(0., 1000., 0.).looking_at(Vec3::new(300.0, 0.0, 300.0), Vec3::Y),
        ));
        warn!("Setting up debug window done");
    }
}

#![warn(missing_docs)]
#![deny(
    clippy::disallowed_types,
    clippy::await_holding_refcell_ref,
    clippy::await_holding_lock
)]
#![allow(clippy::type_complexity)]

//! The clientside of OpenCubeGame
mod debugcam;
pub mod network;
mod voronoi_renderer;
pub mod voxel;

use bevy::a11y::AccessibilityPlugin;
use bevy::audio::AudioPlugin;
use bevy::core_pipeline::CorePipelinePlugin;
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::gltf::GltfPlugin;
use bevy::input::InputPlugin;
use bevy::pbr::PbrPlugin;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::RenderPlugin;
use bevy::scene::ScenePlugin;
use bevy::sprite::SpritePlugin;
use bevy::text::TextPlugin;
use bevy::time::TimePlugin;
use bevy::ui::UiPlugin;
use bevy::window::{ExitCondition, PresentMode};
use bevy::winit::WinitPlugin;
use ocg_common::config::{GameConfig, ServerConfig};
use ocg_common::network::thread::NetworkThread;
use ocg_common::prelude::*;
use ocg_common::{builtin_game_registries, GameServer};
use ocg_schemas::dependencies::uuid::Uuid;
use ocg_schemas::registries::GameRegistries;
use ocg_schemas::schemas::SchemaUuidExt;
use ocg_schemas::{GameSide, OcgExtraData};

use crate::network::NetworkThreadClientState;

/// An [`OcgExtraData`] implementation containing the client-side data for the game engine.
#[derive(Resource)]
pub struct ClientData {
    /// Shared client/server registries.
    pub shared_registries: GameRegistries,
}

impl OcgExtraData for ClientData {
    type ChunkData = voxel::ClientChunkData;
    type GroupData = ();
}

/// The entry point to the client executable
pub fn client_main() {
    // Unset the manifest dir to make bevy load assets from the workspace root
    std::env::set_var("CARGO_MANIFEST_DIR", "");

    let default_registries = builtin_game_registries();
    let game_config = GameConfig {
        server: ServerConfig {
            server_title: String::from("Integrated server"),
            ..Default::default()
        },
    };
    let game_config = GameConfig::new_handle(game_config);
    let integ_server = GameServer::new(game_config).expect("Could not start integrated server");
    integ_server.set_paused(false);
    let server_pipe = integ_server
        .create_local_connection()
        .expect("Could not create an integrated server connection");

    let net_thread = NetworkThread::new(GameSide::Client, NetworkThreadClientState::default);

    struct IntegBootstrap {
        registries: GameRegistries,
    }

    let bootstrap_data = net_thread
        .exec_async_await(|state| {
            Box::pin(async move {
                NetworkThreadClientState::connect_locally(
                    state,
                    server_pipe.await.context("integ_server.create_local_connection")?,
                )
                .await
                .context("NetworkThreadClientState::connect_locally")?;
                let bootstrap_request = state
                    .borrow()
                    .server_auth_rpc()
                    .context("Missing auth endpoint")?
                    .bootstrap_game_data_request();
                let bootstrap_response = bootstrap_request
                    .send()
                    .promise
                    .await
                    .context("Failed bootstrap request to the integrated server")?;
                let bootstrap_response = bootstrap_response.get()?.get_data()?;
                let uuid = Uuid::read_from_message(&bootstrap_response.get_universe_id()?);
                let registries = default_registries.clone_with_serialized_ids(&bootstrap_response)?;
                let nblocks = registries.block_types.len();
                info!("Joining server world {uuid} with {nblocks} block types.");

                anyhow::Ok(IntegBootstrap { registries })
            })
        })
        .expect("Could not connect the client to the integrated server")
        .expect("Could not connect the client to the integrated server");

    let client_data = ClientData {
        shared_registries: bootstrap_data.registries,
    };

    net_thread
        .exec_async(|state| {
            Box::pin(async move {
                let auth_rpc = state.borrow().server_auth_rpc().cloned();
                if let Some(auth_rpc) = auth_rpc {
                    let mut rq = auth_rpc.send_chat_message_request();
                    rq.get().set_text("Hello in-process networking!");
                    let _ = rq.send().promise.await;
                }
            })
        })
        .expect("Could not send message");

    // let integ_conn = integ_server

    let mut app = App::new();
    // Bevy Base
    app.add_plugins(TaskPoolPlugin::default())
        .add_plugins(TypeRegistrationPlugin)
        .add_plugins(FrameCountPlugin)
        .add_plugins(TimePlugin)
        .add_plugins(TransformPlugin)
        .add_plugins(HierarchyPlugin)
        .add_plugins(DiagnosticsPlugin)
        .add_plugins(InputPlugin)
        .add_plugins(WindowPlugin {
            primary_window: Some(Window {
                title: "OpenCubeGame".to_string(),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            exit_condition: ExitCondition::OnPrimaryClosed,
            close_when_requested: true,
        })
        .add_plugins(AccessibilityPlugin)
        .add_plugins(AssetPlugin::default())
        .add_plugins(ScenePlugin)
        .add_plugins(WinitPlugin::default())
        .add_plugins(RenderPlugin::default())
        .add_plugins(ImagePlugin::default())
        .add_plugins(PipelinedRenderingPlugin)
        .add_plugins(CorePipelinePlugin)
        .add_plugins(SpritePlugin)
        .add_plugins(TextPlugin)
        .add_plugins(UiPlugin)
        .add_plugins(PbrPlugin::default())
        .add_plugins(AudioPlugin::default())
        .add_plugins(GilrsPlugin)
        .add_plugins(AnimationPlugin)
        .add_plugins(GltfPlugin::default())
        .add_plugins(debugcam::PlayerPlugin);

    app.insert_resource(client_data);

    app.add_plugins(debug_window::DebugWindow);

    app.run();
}

mod debug_window {
    use std::time::Instant;

    use bevy::log;
    use bevy::prelude::*;
    use ocg_common::voxel::biomes::setup_basic_biomes;
    use ocg_common::voxel::generator::StdGenerator;
    use ocg_common::voxel::generator::WORLD_SIZE_XZ;
    use ocg_common::voxel::generator::WORLD_SIZE_Y;
    use ocg_schemas::coordinates::AbsChunkPos;
    use ocg_schemas::dependencies::itertools::iproduct;
    use ocg_schemas::voxel::biome::BiomeRegistry;
    use ocg_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};

    use crate::voxel::meshgen::mesh_from_chunk;
    use crate::voxel::{ClientChunk, ClientChunkGroup};
    use crate::{voronoi_renderer, ClientData};

    pub struct DebugWindow;

    impl Plugin for DebugWindow {
        fn build(&self, app: &mut App) {
            app.add_systems(Startup, debug_window_setup);
        }
    }

    fn debug_window_setup(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        mut images: ResMut<Assets<Image>>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        client_data: Res<ClientData>,
    ) {
        log::warn!("Setting up debug window");
        let font: Handle<Font> = asset_server.load("fonts/cascadiacode.ttf");

        let white_material = materials.add(StandardMaterial {
            base_color: Color::GRAY,
            ..default()
        });
        let block_reg = &client_data.shared_registries.block_types;
        let (empty, _) = block_reg.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();

        let mut biome_reg = BiomeRegistry::default();
        setup_basic_biomes(&mut biome_reg);
        let biome_reg = biome_reg;

        let mut generator = StdGenerator::new(123456789, WORLD_SIZE_XZ * 2, WORLD_SIZE_XZ as u32 * 4);
        generator.generate_world_biomes(&biome_reg);
        let world_size_blocks = generator.size_blocks_xz() as usize;
        let img_handle = images.add(voronoi_renderer::draw_voronoi(
            &generator,
            &biome_reg,
            world_size_blocks,
            world_size_blocks,
        ));

        let start = Instant::now();

        let mut test_chunks = ClientChunkGroup::new();
        for (cx, cy, cz) in iproduct!(
            -WORLD_SIZE_XZ..=WORLD_SIZE_XZ,
            -WORLD_SIZE_Y..=WORLD_SIZE_Y,
            -WORLD_SIZE_XZ..=WORLD_SIZE_XZ
        ) {
            let cpos = AbsChunkPos::new(cx, cy, cz);
            let mut chunk = ClientChunk::new(BlockEntry::new(empty, 0), Default::default());
            generator.generate_chunk(cpos, &mut chunk.blocks, block_reg, &biome_reg);
            test_chunks.chunks.insert(cpos, chunk);
        }
        for (pos, _) in test_chunks.chunks.iter() {
            let chunks = &test_chunks.get_neighborhood_around(*pos).transpose_option();
            if let Some(chunks) = chunks {
                let chunk_mesh = mesh_from_chunk(block_reg, chunks).unwrap();

                commands.spawn(PbrBundle {
                    mesh: meshes.add(chunk_mesh),
                    material: white_material.clone(),
                    transform: Transform::from_xyz(0.0, 0.0, 0.0),
                    ..default()
                });
            }
        }

        let duration = start.elapsed();
        println!("chunk generation took {:?}", duration);

        commands.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                shadows_enabled: false,
                illuminance: 1000.0,
                ..default()
            },
            transform: Transform::from_xyz(0., 1000., 0.).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            ..default()
        });

        commands
            .spawn(NodeBundle {
                style: Style {
                    width: Val::Percent(25.0),
                    height: Val::Percent(25.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                background_color: Color::CRIMSON.into(),
                ..default()
            })
            .with_children(|parent| {
                parent.spawn(TextBundle::from_section(
                    "Hello OCG",
                    TextStyle {
                        font: font.clone(),
                        font_size: 75.0,
                        color: Color::rgb(0.9, 0.9, 0.9),
                    },
                ));
                log::warn!("Child made");
            });

        commands.spawn(ImageBundle {
            image: UiImage::new(img_handle),
            style: Style {
                width: Val::Px(100.0),
                height: Val::Px(100.0),
                flex_shrink: 0.0,
                ..default()
            },
            ..default()
        });
        log::warn!("Setting up debug window done");
    }
}

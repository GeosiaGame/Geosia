#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]
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
use ocg_common::GameServer;
use ocg_schemas::{GameSide, OcgExtraData};

use crate::network::NetworkThreadClientState;

/// An [`OcgExtraData`] implementation containing the client-side data for the game engine.
pub struct ClientData;

impl OcgExtraData for ClientData {
    type ChunkData = voxel::ClientChunkData;
    type GroupData = ();
}

/// The entry point to the client executable
pub fn client_main() {
    // Unset the manifest dir to make bevy load assets from the workspace root
    std::env::set_var("CARGO_MANIFEST_DIR", "");

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
    net_thread
        .exec_async_await(|state| {
            Box::pin(async move {
                NetworkThreadClientState::connect_locally(
                    state,
                    server_pipe.await.context("integ_server.create_local_connection")?,
                )
                .await
                .context("NetworkThreadClientState::connect_locally")
            })
        })
        .expect("Could not connect the the client to the integrated server")
        .expect("Could not connect the the client to the integrated server");

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

    app.add_plugins(debug_window::DebugWindow);

    app.run();
}

mod debug_window {
    use std::time::Instant;

    use bevy::log;
    use bevy::prelude::*;
    use ocg_common::voxel::biomes::setup_basic_biomes;
    use ocg_common::voxel::blocks::setup_basic_blocks;
    use ocg_common::voxel::generator::StdGenerator;
    use ocg_common::voxel::generator::WORLD_SIZE_XZ;
    use ocg_common::voxel::generator::WORLD_SIZE_Y;
    use ocg_schemas::coordinates::AbsChunkPos;
    use ocg_schemas::dependencies::itertools::iproduct;
    use ocg_schemas::voxel::biome::BiomeRegistry;
    use ocg_schemas::voxel::voxeltypes::{BlockEntry, BlockRegistry, EMPTY_BLOCK_NAME};

    use crate::voronoi_renderer;
    use crate::voxel::meshgen::mesh_from_chunk;
    use crate::voxel::{ClientChunk, ClientChunkGroup};

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
    ) {
        log::warn!("Setting up debug window");
        let font: Handle<Font> = asset_server.load("fonts/cascadiacode.ttf");

        let white_material = materials.add(StandardMaterial {
            base_color: Color::GRAY,
            ..default()
        });

        let mut block_reg = BlockRegistry::default();
        setup_basic_blocks(&mut block_reg);
        let block_reg = block_reg;
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
            generator.generate_chunk(cpos, &mut chunk.blocks, &block_reg, &biome_reg);
            test_chunks.chunks.insert(cpos, chunk);
        }
        for (pos, _) in test_chunks.chunks.iter() {
            let chunks = &test_chunks.get_neighborhood_around(*pos).transpose_option();
            if let Some(chunks) = chunks {
                let chunk_mesh = mesh_from_chunk(&block_reg, chunks).unwrap();

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

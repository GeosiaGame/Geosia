#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]

//! The clientside of OpenCubeGame
pub mod voxel;
mod debugcam;

use bevy::a11y::AccessibilityPlugin;
use bevy::audio::AudioPlugin;
use bevy::core_pipeline::CorePipelinePlugin;
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::gltf::GltfPlugin;
use bevy::input::InputPlugin;
use bevy::log::LogPlugin;
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

/// The entry point to the client executable
pub fn client_main() {
    // Unset the manifest dir to make bevy load assets from the workspace root
    std::env::set_var("CARGO_MANIFEST_DIR", "");

    let mut app = App::new();
    // Bevy Base
    app.add_plugins(LogPlugin::default())
        .add_plugins(TaskPoolPlugin::default())
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
        .add_plugins(WinitPlugin)
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
    use ocg_schemas::voxel::biome::biome_map::BiomeMap;
    use ocg_schemas::voxel::biome::biome_picker::BiomeGenerator;
    use ocg_schemas::voxel::voxeltypes::{BlockEntry, BlockRegistry, EMPTY_BLOCK_NAME};

    use crate::voxel::meshgen::mesh_from_chunk;
    use crate::voxel::{ClientChunk, ClientChunkGroup};

    pub struct DebugWindow;

    impl Plugin for DebugWindow {
        fn build(&self, app: &mut App) {
            app.insert_resource(BiomeMap::default())
                .insert_resource(BiomeRegistry::default())
                .add_systems(Startup, debug_window_setup);
        }
    }

    fn debug_window_setup(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        biome_map: ResMut<BiomeMap>,
        mut biome_reg: ResMut<BiomeRegistry>,
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

        setup_basic_biomes(&mut biome_reg);

        let mut generator = StdGenerator::new(0, biome_map, BiomeGenerator::new(0));

        let mut test_chunks = ClientChunkGroup::new();
        for (cx, cy, cz) in iproduct!(-WORLD_SIZE_XZ..=WORLD_SIZE_XZ, -WORLD_SIZE_Y..=WORLD_SIZE_Y, -WORLD_SIZE_XZ..=WORLD_SIZE_XZ) {
            let cpos = AbsChunkPos::new(cx, cy, cz);
            let mut chunk = ClientChunk::new(BlockEntry::new(empty, 0), Default::default());
            generator.generate_chunk(cpos, &mut chunk.blocks, &block_reg, &biome_reg);
            test_chunks.chunks.insert(cpos, chunk);
        }
        for (pos, _) in test_chunks.chunks.iter() {
            let chunks = &test_chunks
            .get_neighborhood_around(*pos)
            .transpose_option();
            if chunks.is_some() {
                let c00mesh = mesh_from_chunk(
                    &block_reg,
                    &chunks.as_ref().unwrap(),
                )
                .unwrap();
    
                commands.spawn(PbrBundle {
                    mesh: meshes.add(c00mesh),
                    material: white_material.clone(),
                    transform: Transform::from_xyz(0.0, 0.0, 0.0),
                    ..default()
                });
            }
        }
        
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
        log::warn!("Setting up debug window done");
    }
}

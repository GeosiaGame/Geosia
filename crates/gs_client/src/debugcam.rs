//Copyright 2020 Spencer Burris
//
//Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.
//
//THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

use bevy::asset::RenderAssetUsages;
use bevy::color::palettes::tailwind;
use bevy::image::ImageSampler;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_egui::egui::{Align2, TextureOptions};
use bevy_egui::input::egui_wants_any_input;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, EguiTextureHandle};
use image::{ImageBuffer, Rgba};
use gs_common::network::transport::PacketWrapper;
use gs_common::raycast::{RaycastContext, raycast};
use gs_common::voxel::plugin::{BlockRegistryHolder, CHUNK_LOAD_RADIUS};
use gs_schemas::actions::{BlockAction, PositionData};
use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos, RelChunkPos, WorldPos};
use gs_schemas::dependencies::itertools::iproduct;
use gs_schemas::raycast::{RaycastHitMask, RaycastResult, RaycastSpec};
use gs_schemas::schemas::network_capnp::{PacketId, player_move_request};
use gs_schemas::schemas::{CapnpExt, new_packet_builder};
use gs_schemas::voxel::voxeltypes::EMPTY_BLOCK;

use crate::network::AuthenticatedNetworkClient;
use crate::prelude::*;
use crate::states::{ClientAppState, InGameSystemSet, in_game};
use crate::ui::{IsCursorGrabbed, SetGrabMode};
use crate::voxel::ClientVoxelUniverse;

/// Mouse sensitivity and movement speed
#[derive(Resource)]
pub struct MovementSettings {
    pub sensitivity: f32,
    pub speed: f32,
}

impl Default for MovementSettings {
    fn default() -> Self {
        Self {
            sensitivity: 0.00012,
            speed: 24.,
        }
    }
}

/// Key configuration
#[derive(Resource)]
pub struct KeyBindings {
    pub move_forward: KeyCode,
    pub move_backward: KeyCode,
    pub move_left: KeyCode,
    pub move_right: KeyCode,
    pub move_ascend: KeyCode,
    pub move_descend: KeyCode,
    pub open_chat: KeyCode,
    pub toggle_grab_cursor: KeyCode,
    pub place_block: MouseButton,
    pub break_block: MouseButton,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            move_forward: KeyCode::KeyW,
            move_backward: KeyCode::KeyS,
            move_left: KeyCode::KeyA,
            move_right: KeyCode::KeyD,
            move_ascend: KeyCode::Space,
            move_descend: KeyCode::ShiftLeft,
            open_chat: KeyCode::KeyT,
            toggle_grab_cursor: KeyCode::Escape,
            place_block: MouseButton::Left,
            break_block: MouseButton::Right,
        }
    }
}

/// Used in queries when you want flycams and not other cameras
/// A marker component used in queries when you want flycams and not other cameras
#[derive(Component)]
pub struct FlyCam;

#[derive(Component)]
pub struct BiomeText;

#[derive(Component)]
pub struct PositionText;

/// Grabs the cursor when game first starts
fn initial_grab_cursor(state: Res<IsCursorGrabbed>, mut commands: Commands) {
    if !**state {
        commands.trigger(SetGrabMode(true));
    }
}

/// Spawns the `Camera3dBundle` to be controlled
fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 12.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        FlyCam,
    ));
}

/// Handles input for movement
fn player_move(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    primary_window: Query<&CursorOptions, With<PrimaryWindow>>,
    settings: Res<MovementSettings>,
    key_bindings: Res<KeyBindings>,
    mut camera_query: Query<(&FlyCam, &mut Transform)>, //    mut query: Query<&mut Transform, With<FlyCam>>,
    mut text_writer: TextUiWriter,
    mut set: ParamSet<(Query<Entity, With<BiomeText>>, Query<Entity, With<PositionText>>)>,
) {
    if let Ok(cursor_options) = primary_window.single() {
        let mut camera_pos = Vec3::ZERO;
        let mut camera_angle = Quat::IDENTITY;
        for (_camera, mut transform) in camera_query.iter_mut() {
            let mut velocity = Vec3::ZERO;
            let local_z = transform.local_z();
            let forward = -Vec3::new(local_z.x, 0., local_z.z);
            let right = Vec3::new(local_z.z, 0., -local_z.x);

            for key in keys.get_pressed() {
                match cursor_options.grab_mode {
                    CursorGrabMode::None => (),
                    _ => {
                        let key = *key;
                        if key == key_bindings.move_forward {
                            velocity += forward;
                        } else if key == key_bindings.move_backward {
                            velocity -= forward;
                        } else if key == key_bindings.move_left {
                            velocity -= right;
                        } else if key == key_bindings.move_right {
                            velocity += right;
                        } else if key == key_bindings.move_ascend {
                            velocity += Vec3::Y;
                        } else if key == key_bindings.move_descend {
                            velocity -= Vec3::Y;
                        }
                    }
                }

                velocity = velocity.normalize_or_zero();

                transform.translation += velocity * time.delta_secs() * settings.speed;
            }
            camera_pos = transform.translation;
            camera_angle = transform.rotation;
        }
        /*
        for mut text in &mut set.p0() {
            let i_camera_pos = camera_pos.as_ivec3();
            let biomes = biome_map.biome_map.get(&[i_camera_pos.x, i_camera_pos.z]);
            if biomes.is_some() {
                let mut t = String::new();
                for (i, biome) in biomes.unwrap().iter().enumerate() {
                    t += format!("\n  biome #{i}:{{id: {0}, weight: {1}}}", biome.lookup(&biome_registry).unwrap(), biome.weight).as_str();
                }
                text.sections[1].value = t;
            }
        }
        */
        for text in &set.p1() {
            *text_writer.text(text, 1) = camera_pos.to_string();
            let euler = camera_angle.to_euler(EulerRot::XYZ);
            let euler = (euler.0 * 1.0, euler.1 * 1.0, euler.2 * 1.0);
            *text_writer.text(text, 3) = format!("{:?}", euler);
        }
    } else {
        warn!("Primary window not found for `player_move`!");
    }
}

/// Handles looking around if cursor is locked
fn player_look(
    settings: Res<MovementSettings>,
    primary_window: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<&mut Transform, With<FlyCam>>,
) {
    if let Ok((window, cursor_options)) = primary_window.single() {
        for mut transform in camera_query.iter_mut() {
            let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
            match cursor_options.grab_mode {
                CursorGrabMode::None => (),
                _ => {
                    // Using smallest of height or width ensures equal vertical and horizontal sensitivity
                    let window_scale = window.height().min(window.width());
                    pitch -= (settings.sensitivity * motion.delta.y * window_scale).to_radians();
                    yaw -= (settings.sensitivity * motion.delta.x * window_scale).to_radians();
                }
            }

            pitch = pitch.clamp(-1.54, 1.54);

            // Order is important to prevent unintended roll
            transform.rotation = Quat::from_axis_angle(Vec3::Y, yaw) * Quat::from_axis_angle(Vec3::X, pitch);
        }
    } else {
        warn!("Primary window not found for `player_look`!");
    }
}

/// Handles input for actions
fn player_action(
    authenticated_client: Res<AuthenticatedNetworkClient>,
    mut voxels: Query<&mut ClientVoxelUniverse>,
    block_reg: Res<BlockRegistryHolder>,
    _: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    primary_window: Query<&CursorOptions, With<PrimaryWindow>>,
    key_bindings: Res<KeyBindings>,
    mut camera_query: Query<(&FlyCam, &Transform)>,
) {
    if let Ok(cursor_options) = primary_window.single() {
        for (_camera, transform) in camera_query.iter_mut() {
            for &button in mouse.get_just_pressed() {
                match cursor_options.grab_mode {
                    CursorGrabMode::None => (),
                    _ => {
                        if button == key_bindings.place_block {
                            let pos = transform.translation.floor();
                            let offset = transform.translation - pos;
                            let pos = AbsBlockPos::from_ivec3(pos.as_ivec3());
                            in_game::ingame_send_block_change(
                                &authenticated_client,
                                &mut voxels,
                                &block_reg,
                                PositionData {
                                    position: pos,
                                    offset,
                                    look: transform.forward().into(),
                                },
                                BlockAction::PlaceBlock(),
                            );
                        } else if button == key_bindings.break_block {
                            let Vec3 { x, y, z } = transform.translation;
                            let pos = AbsBlockPos::new(x.floor() as i32, y.floor() as i32, z.floor() as i32);
                            let offset = Vec3 {
                                x: x - pos.x as f32,
                                y: y - pos.y as f32,
                                z: z - pos.z as f32,
                            };
                            in_game::ingame_send_block_change(
                                &authenticated_client,
                                &mut voxels,
                                &block_reg,
                                PositionData {
                                    position: pos,
                                    offset,
                                    look: transform.forward().into(),
                                },
                                BlockAction::BreakBlock(),
                            );
                        }
                    }
                }
            }
        }
    } else {
        warn!("Primary window not found for `player_action`!");
    }
}

fn player_movement_packet_sender(
    authenticated_client: Res<AuthenticatedNetworkClient>,
    camera_query: Populated<&Transform, (With<FlyCam>, Changed<Transform>)>,
    mut previous_transform: Local<Transform>,
) -> BevyResult<()> {
    let transform = camera_query.single()?;
    if transform == &*previous_transform {
        return Ok(());
    }
    *previous_transform = *transform;
    // To be replaced with a real universe position once we keep track of that in a client player entity.
    let world_pos = WorldPos::from_dvec3(transform.translation.as_dvec3());

    let mut request = new_packet_builder::<player_move_request::Owned>();
    let mut root = request.init_root();
    root.set_id(PacketId::MovePlayer);
    root.set_timestamp_ms(authenticated_client.packet_timestamp());
    let mut root = root.init_payload();
    root.set_tick(0); // TODO send the actual client tick
    world_pos.write_to_message(&mut root.reborrow().init_position());
    transform
        .rotation
        .write_to_message(&mut root.reborrow().init_rotation());
    let request = PacketWrapper::from(request);
    let _ = authenticated_client.main_c2s_stream.send_packet(request);

    Ok(())
}

#[derive(Resource)]
struct DebugGizmoToggles {
    local_coordinates: bool,
    current_chunk: bool,
    raycast: bool,
    generated_chunk_minimap: bool,
}

impl Default for DebugGizmoToggles {
    fn default() -> Self {
        Self {
            local_coordinates: false,
            current_chunk: true,
            raycast: false,
            generated_chunk_minimap: true,
        }
    }
}

fn gizmo_toggles(
    camera_query: Query<&Transform, With<FlyCam>>,
    mut ui: EguiContexts,
    mut toggles: ResMut<DebugGizmoToggles>,
    cursor_grabbed: Res<IsCursorGrabbed>,
) {
    use bevy_egui::egui;
    if camera_query.is_empty() {
        return;
    }
    let Ok(ctx) = ui.ctx_mut() else { return };

    let toggles = &mut *toggles;
    egui::Window::new("Debug gizmos")
        .collapsible(true)
        .resizable(false)
        .interactable(!**cursor_grabbed)
        .anchor(Align2::RIGHT_BOTTOM, egui::vec2(0.0, 0.0))
        .auto_sized()
        .show(ctx, move |ui| {
            ui.checkbox(&mut toggles.local_coordinates, "Local Coords");
            ui.checkbox(&mut toggles.current_chunk, "Current Chunk");
            ui.checkbox(&mut toggles.raycast, "Raycast");
            ui.checkbox(&mut toggles.generated_chunk_minimap, "Generated Chunk Minimap");
        });
}

fn xyz_gizmo(camera_query: Query<&Transform, With<FlyCam>>, mut gizmos: Gizmos, toggles: Res<DebugGizmoToggles>) {
    if !toggles.local_coordinates {
        return;
    }
    let len = 0.5;
    let Ok(&camera) = camera_query.single() else {
        return;
    };
    let arrow_start = camera.transform_point(vec3(0.0, 0.0, -4.0));
    gizmos.sphere(arrow_start, len, tailwind::GRAY_700);
    gizmos.arrow(arrow_start, arrow_start + len * Vec3::X, tailwind::RED_600);
    gizmos.arrow(arrow_start, arrow_start + len * Vec3::Y, tailwind::GREEN_600);
    gizmos.arrow(arrow_start, arrow_start + len * Vec3::Z, tailwind::BLUE_600);
}

fn cur_chunk_gizmo(
    camera_query: Query<&Transform, With<FlyCam>>,
    voxels: Query<&ClientVoxelUniverse>,
    bregistry: Option<Res<BlockRegistryHolder>>,
    mut gizmos: Gizmos,
    toggles: Res<DebugGizmoToggles>,
) {
    if !toggles.current_chunk {
        return;
    }
    let Ok(&camera) = camera_query.single() else {
        return;
    };
    let Ok(voxels) = voxels.single() else {
        return;
    };
    let Some(bregistry) = bregistry else {
        return;
    };
    let camera_zero: Vec3A = camera.transform_point(Vec3::ZERO).into();

    let curcpos = AbsChunkPos::from(WorldPos::from_vec3(camera_zero).as_blockpos());
    let curchunk = voxels.loaded_chunks().get_chunk(curcpos);
    if let Some(chunk) = curchunk {
        let chunk = chunk.read();
        for (pos, entry) in chunk.blocks.iter_with_coords() {
            let block = bregistry.lookup_id_to_object(entry.id).unwrap_or(&EMPTY_BLOCK);
            if !block.has_drawable_mesh {
                continue;
            }
            let apos = curcpos.block_pos(pos).block_center().as_vec3();
            gizmos.cube(Transform::from_translation(apos), block.representative_color);
        }
    }
}

fn lookat_gizmo(
    camera_query: Query<&Transform, With<FlyCam>>,
    voxels: Query<&ClientVoxelUniverse>,
    bregistry: Option<Res<BlockRegistryHolder>>,
    mut gizmos: Gizmos,
    toggles: Res<DebugGizmoToggles>,
) {
    if !toggles.raycast {
        return;
    }
    let limit = 64.0;
    let Ok(&camera) = camera_query.single() else {
        return;
    };
    let Ok(voxels) = voxels.single() else {
        return;
    };
    let Some(bregistry) = bregistry else {
        return;
    };
    let ray_ctx = RaycastContext {
        block_registry: Some(&bregistry),
        voxel_world: Some(voxels),
    };
    let camera_zero: Vec3A = camera.transform_point(Vec3::ZERO).into();
    let ray_spec = RaycastSpec {
        start: WorldPos::from_vec3(camera_zero),
        direction: camera.forward().into(),
        distance_limit: limit,
        hit_mask: RaycastHitMask::all(),
    };

    let rc = raycast(&ray_ctx, &ray_spec);
    let RaycastResult::BlockHit(rc) = rc else {
        let limit_sphere = camera.transform_point(vec3(0.0, 0.0, -limit));
        gizmos.sphere(limit_sphere, 0.5, tailwind::RED_600);
        return;
    };
    let zero_cube = rc.position.as_vec3();
    let mid_cube = rc.position.block_center().as_vec3();
    gizmos.cube(
        Transform::from_translation(mid_cube).with_scale(Vec3::splat(1.1)),
        tailwind::AMBER_500,
    );
    gizmos.arrow(mid_cube, mid_cube + Vec3::from(rc.face.as_vec()), tailwind::AMBER_400);
    gizmos.sphere(zero_cube + Vec3::from(rc.f32_offset), 0.1, tailwind::GREEN_800);
}

fn generated_chunk_minimap_gizmo(
    camera_query: Query<&Transform, With<FlyCam>>,
    voxels: Query<&ClientVoxelUniverse>,
    toggles: Res<DebugGizmoToggles>,

    mut image_assets: ResMut<Assets<Image>>,
    images: Res<Images>,
    mut rendered_texture_id: Local<bevy_egui::egui::TextureId>,
    mut is_initialized: Local<bool>,

    mut ui: EguiContexts,
) {
    use bevy_egui::egui;

    if !toggles.generated_chunk_minimap {
        return;
    }

    let image_asset_id = images.minimap_image.id();
    if !*is_initialized {
        *is_initialized = true;
        *rendered_texture_id = ui.add_image(EguiTextureHandle::Weak(image_asset_id));
    }
    // Obtain a mutable reference to the Image asset.
    let Some(mut image) = image_assets.get_mut(image_asset_id) else {
        return;
    };
    if image.data.is_none() {
        let buf = ImageBuffer::from_pixel(MINIMAP_IMAGE_SIZE, MINIMAP_IMAGE_SIZE, BLANK_COLOR);
        image.data = Some(buf.into_raw());
    } else {
        image.clear(&BLANK_COLOR.0);
    }

    let Ok(ctx) = ui.ctx_mut() else { return };

    let Ok(camera) = camera_query.single() else {
        return;
    };
    let Ok(voxels) = voxels.single() else {
        return;
    };
    let camera_zero: Vec3A = camera.translation.into();

    let current_c_pos = AbsChunkPos::from(WorldPos::from_vec3(camera_zero).as_blockpos());
    for (x, y, z) in iproduct!(-MINIMAP_IMAGE_HALF_SIZE..=MINIMAP_IMAGE_HALF_SIZE, -MINIMAP_IMAGE_HALF_SIZE..=MINIMAP_IMAGE_HALF_SIZE, -MINIMAP_IMAGE_HALF_SIZE..=MINIMAP_IMAGE_HALF_SIZE) {
        let p_x = (current_c_pos.x + x + MINIMAP_IMAGE_HALF_SIZE) as u32;
        let p_y = (current_c_pos.z + z + MINIMAP_IMAGE_HALF_SIZE) as u32;
        if p_x >= MINIMAP_IMAGE_SIZE || p_y >= MINIMAP_IMAGE_SIZE {
            continue;
        }

        let current_chunk = voxels.loaded_chunks().get_chunk(current_c_pos + RelChunkPos::new(x, y, z));
        if current_chunk.is_some() {
            let LinearRgba {red: luma, .. } = image.get_color_at(p_x, p_y).expect("invalid color").to_linear();

            const SINGLE_STEP_LUMA: f32 = 1.0 / CHUNK_LOAD_RADIUS as f32;
            image.set_color_at(p_x, p_y, LinearRgba::gray(luma + SINGLE_STEP_LUMA).into()).expect("invalid color");
        }
    }

    egui::Window::new("Generated Chunk Minimap")
        .collapsible(true)
        .resizable(false)
        .anchor(Align2::RIGHT_TOP, egui::vec2(0.0, 0.0))
        .auto_sized()
        .show(ctx, move |ui| {
            ui.add(egui::Image::new(
                egui::load::SizedTexture::new(
                    *rendered_texture_id,
                    [MINIMAP_IMAGE_SIZEF, MINIMAP_IMAGE_SIZEF]
                ))
                .show_loading_spinner(true)
                .fit_to_original_size(MINIMAP_DISPLAY_FACTOR)
                .texture_options(TextureOptions::NEAREST)
            );
        });
}

fn cursor_grab(
    keys: Res<ButtonInput<KeyCode>>,
    key_bindings: Res<KeyBindings>,
    state: Res<IsCursorGrabbed>,
    mut commands: Commands,
) {
    if keys.just_pressed(key_bindings.toggle_grab_cursor) {
        commands.trigger(SetGrabMode(!**state));
    }
}

fn spawn_debug_text(asset_server: Res<AssetServer>, mut commands: Commands) {
    let font: Handle<Font> = asset_server.load("fonts/cascadiacode.ttf");
    commands
        .spawn((
            Text::new("Current Biome: "),
            TextFont::from(font.clone()).with_font_size(15.0),
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
            BiomeText,
        ))
        .with_child((
            TextSpan::new(""),
            TextFont::from(font.clone()).with_font_size(15.0),
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
        ));
    commands
        .spawn((
            Text::new("\nCurrent Position:"),
            TextFont::from(font.clone()).with_font_size(15.0),
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
            BiomeText,
            PositionText,
        ))
        .with_children(|b| {
            b.spawn((
                TextSpan::new(""),
                TextFont::from(font.clone()).with_font_size(15.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
            b.spawn((
                TextSpan::new("\nCurrent Rotation:"),
                TextFont::from(font.clone()).with_font_size(15.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
            b.spawn((
                TextSpan::new(""),
                TextFont::from(font.clone()).with_font_size(15.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
        });
}

const MINIMAP_IMAGE_HALF_SIZE: i32 = CHUNK_LOAD_RADIUS * 2;
const MINIMAP_IMAGE_SIZE: u32 = MINIMAP_IMAGE_HALF_SIZE as u32 * 2;
const MINIMAP_IMAGE_SIZEF: f32 = MINIMAP_IMAGE_SIZE as f32;
const MINIMAP_DISPLAY_FACTOR: f32 = 16.0;
const BLANK_COLOR: Rgba<u8> = Rgba([0x00, 0x00, 0x00, 0x00]);

#[derive(Resource)]
struct Images {
    minimap_image: Handle<Image>,
}

impl FromWorld for Images {
    fn from_world(world: &mut World) -> Self {
        let mut image_assets = world.get_resource_mut::<Assets<Image>>().unwrap();

        let buf = ImageBuffer::from_pixel(MINIMAP_IMAGE_SIZE, MINIMAP_IMAGE_SIZE, BLANK_COLOR);
        let mut image = Image::new(
            Extent3d {
                width: buf.width(),
                height: buf.height(),
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            buf.into_raw(),
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default()
        );
        image.sampler = ImageSampler::linear();

        Self {
            minimap_image: image_assets.add(image),
        }
    }
}

/// Contains everything needed to add first-person fly camera behavior to your game
pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementSettings>()
            .init_resource::<KeyBindings>()
            .init_resource::<DebugGizmoToggles>()
            .init_resource::<Images>()
            .add_systems(Startup, setup_camera)
            .add_systems(OnEnter(ClientAppState::InGame), initial_grab_cursor)
            .add_systems(OnEnter(ClientAppState::InGame), spawn_debug_text)
            .add_systems(
                Update,
                player_move.run_if(not(egui_wants_any_input)).in_set(InGameSystemSet),
            )
            .add_systems(
                Update,
                player_look.run_if(not(egui_wants_any_input)).in_set(InGameSystemSet),
            )
            .add_systems(
                Update,
                player_action.run_if(not(egui_wants_any_input)).in_set(InGameSystemSet),
            )
            .add_systems(EguiPrimaryContextPass, (gizmo_toggles, generated_chunk_minimap_gizmo).in_set(InGameSystemSet))
            .add_systems(
                Update,
                (xyz_gizmo, cur_chunk_gizmo, lookat_gizmo)
                    .in_set(InGameSystemSet)
                    .after(player_move)
                    .after(player_look)
                    .after(player_action),
            )
            .add_systems(PostUpdate, (player_movement_packet_sender).in_set(InGameSystemSet))
            .add_systems(Update, cursor_grab.in_set(InGameSystemSet));
    }
}

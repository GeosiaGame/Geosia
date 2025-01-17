//Copyright 2020 Spencer Burris
//
//Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.
//
//THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};

use crate::states::{ClientAppState, InGameSystemSet};

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
    pub toggle_grab_cursor: KeyCode,
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
            toggle_grab_cursor: KeyCode::Escape,
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

/// Grabs/ungrabs mouse cursor
fn toggle_grab_cursor(window: &mut Window) {
    match window.cursor_options.grab_mode {
        CursorGrabMode::None => {
            window.cursor_options.grab_mode = CursorGrabMode::Confined;
            window.cursor_options.visible = false;
        }
        _ => {
            window.cursor_options.grab_mode = CursorGrabMode::None;
            window.cursor_options.visible = true;
        }
    }
}

/// Grabs the cursor when game first starts
fn initial_grab_cursor(mut primary_window: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = primary_window.get_single_mut() {
        if window.focused {
            toggle_grab_cursor(&mut window);
        }
    } else {
        warn!("Primary window not found for `initial_grab_cursor`!");
    }
}

/// Spawns the `Camera3dBundle` to be controlled
fn setup_player(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
        FlyCam,
    ));
}

/// Handles keyboard input and movement
fn player_move(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    settings: Res<MovementSettings>,
    key_bindings: Res<KeyBindings>,
    mut camera_query: Query<(&FlyCam, &mut Transform)>, //    mut query: Query<&mut Transform, With<FlyCam>>,
    mut text_writer: TextUiWriter,
    mut set: ParamSet<(Query<Entity, With<BiomeText>>, Query<Entity, With<PositionText>>)>,
) {
    if let Ok(window) = primary_window.get_single() {
        let mut camera_pos = Vec3::ZERO;
        let mut camera_angle = Quat::IDENTITY;
        for (_camera, mut transform) in camera_query.iter_mut() {
            let mut velocity = Vec3::ZERO;
            let local_z = transform.local_z();
            let forward = -Vec3::new(local_z.x, 0., local_z.z);
            let right = Vec3::new(local_z.z, 0., -local_z.x);

            for key in keys.get_pressed() {
                match window.cursor_options.grab_mode {
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
    primary_window: Query<&Window, With<PrimaryWindow>>,
    motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<&mut Transform, With<FlyCam>>,
) {
    if let Ok(window) = primary_window.get_single() {
        for mut transform in camera_query.iter_mut() {
            let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
            match window.cursor_options.grab_mode {
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

fn cursor_grab(
    keys: Res<ButtonInput<KeyCode>>,
    key_bindings: Res<KeyBindings>,
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
) {
    if let Ok(mut window) = primary_window.get_single_mut() {
        if keys.just_pressed(key_bindings.toggle_grab_cursor) {
            toggle_grab_cursor(&mut window);
        }
    } else {
        warn!("Primary window not found for `cursor_grab`!");
    }
}

// Grab cursor when an entity with FlyCam is added
fn initial_grab_on_flycam_spawn(
    mut primary_window: Query<&mut Window, With<PrimaryWindow>>,
    query_added: Query<Entity, Added<FlyCam>>,
) {
    if query_added.is_empty() {
        return;
    }

    if let Ok(window) = &mut primary_window.get_single_mut() {
        toggle_grab_cursor(window);
    } else {
        warn!("Primary window not found for `initial_grab_cursor`!");
    }
}

fn spawn_debug_text(asset_server: Res<AssetServer>, mut commands: Commands) {
    let font: Handle<Font> = asset_server.load("fonts/cascadiacode.ttf");
    commands
        .spawn((
            Text::new("Current Biome:"),
            TextFont::from_font(font.clone()).with_font_size(15.0),
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
            BiomeText,
        ))
        .with_child((
            TextSpan::new(""),
            TextFont::from_font(font.clone()).with_font_size(15.0),
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
        ));
    commands
        .spawn((
            Text::new("\nCurrent Position:"),
            TextFont::from_font(font.clone()).with_font_size(15.0),
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
            BiomeText,
            PositionText,
        ))
        .with_children(|b| {
            b.spawn((
                TextSpan::new(""),
                TextFont::from_font(font.clone()).with_font_size(15.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
            b.spawn((
                TextSpan::new("\nCurrent Rotation:"),
                TextFont::from_font(font.clone()).with_font_size(15.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
            b.spawn((
                TextSpan::new(""),
                TextFont::from_font(font.clone()).with_font_size(15.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
        });
}

/// Contains everything needed to add first-person fly camera behavior to your game
pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementSettings>()
            .init_resource::<KeyBindings>()
            .add_systems(OnEnter(ClientAppState::InGame), setup_player)
            .add_systems(OnEnter(ClientAppState::InGame), initial_grab_cursor)
            .add_systems(OnEnter(ClientAppState::InGame), spawn_debug_text)
            .add_systems(Update, player_move.in_set(InGameSystemSet))
            .add_systems(Update, player_look.in_set(InGameSystemSet))
            .add_systems(Update, cursor_grab.in_set(InGameSystemSet));
    }
}

/// Same as [`PlayerPlugin`] but does not spawn a camera
pub struct NoCameraPlayerPlugin;
impl Plugin for NoCameraPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementSettings>()
            .init_resource::<KeyBindings>()
            .add_systems(OnEnter(ClientAppState::InGame), initial_grab_cursor)
            .add_systems(OnEnter(ClientAppState::InGame), initial_grab_on_flycam_spawn)
            .add_systems(OnEnter(ClientAppState::InGame), spawn_debug_text)
            .add_systems(Update, player_move.in_set(InGameSystemSet))
            .add_systems(Update, player_look.in_set(InGameSystemSet))
            .add_systems(Update, cursor_grab.in_set(InGameSystemSet));
    }
}

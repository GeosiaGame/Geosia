//Copyright 2020 Spencer Burris
//
//Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.
//
//THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
//Code adapted from https://github.com/sburris0/bevy_flycam
//

use bevy::app::{App, Startup, Update};
use bevy::ecs::event::ManualEventReader;
use bevy::input::mouse::MouseMotion;
use bevy::input::Input;
use bevy::log;
use bevy::math::{EulerRot, Quat, Vec3};
use bevy::prelude::{Component, Events, KeyCode, Plugin, Query, Res, ResMut, Resource, Transform, With};
use bevy::time::Time;
use bevy::window::{CursorGrabMode, PrimaryWindow, Window};

const SPEED_FACTOR: f32 = 10.0;
const SENS_FACTOR: f32 = 10000.0;
const CAMERA_PITCH_CLAMP: f32 = 1.5;

/// Keybinds for camera movement
///
/// * `forwards` - move the camera in the direction it is looking in
/// * `backwards` - move the camera in the opposite direction it is looking in
/// * `left` - move the camera to the left
/// * `right` - move the camera to the right
/// * `down` - move the camera downwards
/// * `up` - move the camera upwards
/// * `toggle_mouse_grab` - toggle between grabbing and not grabbing the mouse
/// * `reset_camera` - reset the camera to the default position
///
#[derive(Resource)]
pub struct Keybinds {
    pub forwards: KeyCode,
    pub back: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub down: KeyCode,
    pub up: KeyCode,
    pub toggle_mouse_grab: KeyCode,
    pub reset_camera: KeyCode,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            forwards: KeyCode::W,
            back: KeyCode::S,
            left: KeyCode::A,
            right: KeyCode::D,
            down: KeyCode::ControlLeft,
            up: KeyCode::Space,
            toggle_mouse_grab: KeyCode::Escape,
            reset_camera: KeyCode::R,
        }
    }
}

/// Movement modes for the camera
/// * `AxisAligned` - camera only moves along the X, Y, and Z axes
/// * `AxisIndependent` - camera moves relative to its look vector
///
pub enum CameraMode {
    AxisAligned,
    #[allow(dead_code)]
    AxisIndependent,
}

/// Settings for the camera
/// * `sensitivity` - how much mouse movement effects the camera look vector
/// * `movement_speed` - how fast the camera's position is moved
/// * `camera_mode` - the movement mode the camera should use
///
#[derive(Resource)]
pub struct Settings {
    pub sensitivity: f32,
    pub movement_speed: f32,
    pub camera_mode: CameraMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 0.4,
            movement_speed: 1.0,
            camera_mode: CameraMode::AxisAligned,
        }
    }
}

/// A component to attach to a camera, making it have the FlyCam behavior
#[derive(Component)]
pub struct FlyCam;

/// The plugin to enable FlyCam functionality
pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Keybinds>()
            .init_resource::<Settings>()
            .init_resource::<MouseMotionReader>()
            .add_systems(Startup, grab_startup)
            .add_systems(Update, move_camera_position)
            .add_systems(Update, move_camera_look)
            .add_systems(Update, grab_focus);
    }
}

/// Wrapper struct to utilize an EventReader for MouseMotion as a Resource
#[derive(Resource, Default)]
struct MouseMotionReader {
    event_reader: ManualEventReader<MouseMotion>,
}

/// Handles moving the camera's position
///
/// # Arguments
/// * `keys` - the inputs recorded
/// * `keybinds` - the keybindings for camera control
/// * `settings` - the settings for the camera
/// * `time` - the current time
/// * `window` - the window the camera views
/// * `query` - a query for FlyCam cameras and their Transforms
///
fn move_camera_position(
    keys: Res<Input<KeyCode>>,
    keybinds: Res<Keybinds>,
    settings: Res<Settings>,
    time: Res<Time>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut query: Query<(&FlyCam, &mut Transform)>,
) {
    if let Ok(window) = window.get_single() {
        if window.cursor.grab_mode == CursorGrabMode::None {
            return;
        }

        if keys.pressed(keybinds.reset_camera) {
            for (_, mut transform) in query.iter_mut() {
                transform.translation = Vec3::ZERO;
            }
            return;
        }

        let mut x_dir: i8 = 0;
        let mut y_dir: i8 = 0;
        let mut z_dir: i8 = 0;

        if keys.pressed(keybinds.forwards) {
            x_dir += 1;
        }
        if keys.pressed(keybinds.back) {
            x_dir -= 1;
        }
        if keys.pressed(keybinds.right) {
            z_dir += 1;
        }
        if keys.pressed(keybinds.left) {
            z_dir -= 1;
        }
        if keys.pressed(keybinds.up) {
            y_dir += 1;
        }
        if keys.pressed(keybinds.down) {
            y_dir -= 1;
        }

        for (_, mut transform) in query.iter_mut() {
            let mut velocity = Vec3::ZERO;
            let x_motion;
            let y_motion;
            let z_motion;

            match &settings.camera_mode {
                CameraMode::AxisAligned => {
                    let local_z = transform.local_z();
                    x_motion = -Vec3::new(local_z.x, 0.0, local_z.z);
                    y_motion = Vec3::Y;
                    z_motion = Vec3::new(local_z.z, 0.0, -local_z.x);
                }
                CameraMode::AxisIndependent => {
                    x_motion = -transform.local_z();
                    y_motion = transform.local_y();
                    z_motion = transform.local_x();
                }
            }

            velocity += x_dir as f32 * x_motion;
            velocity += y_dir as f32 * y_motion;
            velocity += z_dir as f32 * z_motion;
            velocity = velocity.normalize_or_zero();

            if velocity != Vec3::ZERO {
                transform.translation += velocity * time.delta_seconds() * SPEED_FACTOR * settings.movement_speed;
            }
        }
    } else {
        log::error!("No window found for camera movement")
    }
}

/// Moves the camera look
///
/// # Arguments
/// * `settings` - the settings for the camera
/// * `window` - the window the camera views
/// * `events` - the mouse motion events to process
/// * `reader` - the event reader for mouse motion events
/// * `query` - a query for FlyCam cameras and their Transforms
///
fn move_camera_look(
    settings: Res<Settings>,
    window: Query<&Window, With<PrimaryWindow>>,
    events: Res<Events<MouseMotion>>,
    mut reader: ResMut<MouseMotionReader>,
    mut query: Query<&mut Transform, With<FlyCam>>,
) {
    if let Ok(window) = window.get_single() {
        for mut transform in query.iter_mut() {
            for motion in reader.event_reader.iter(&events) {
                let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
                if window.cursor.grab_mode != CursorGrabMode::None {
                    // Using smallest of height or width ensures equal vertical and horizontal sensitivity
                    let window_scale = window.height().min(window.width());
                    pitch -= (settings.sensitivity * motion.delta.y * window_scale / SENS_FACTOR).to_radians();
                    yaw -= (settings.sensitivity * motion.delta.x * window_scale / SENS_FACTOR).to_radians();
                }

                pitch = pitch.clamp(-CAMERA_PITCH_CLAMP, CAMERA_PITCH_CLAMP);
                transform.rotation = Quat::from_axis_angle(Vec3::Y, yaw) * Quat::from_axis_angle(Vec3::X, pitch);
            }
        }
    } else {
        log::error!("No window found for camera movement");
    }
}

/// Grabs the mouse on startup
///
/// # Arguments
/// * `query` - a query for Windows which are PrimaryWindows
///
fn grab_startup(mut query: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = query.get_single_mut() {
        grab_window(&mut window, true);
    } else {
        log::error!("No window found for startup mouse grab");
    }
}

/// Grabs and un-grabs the mouse on focus change
///
/// # Arguments
/// * `query` - a query for Windows which are PrimaryWindows
/// * `keys` - the inputs recorded
/// * `keybinds` - the keybindings for camera control
///
fn grab_focus(mut query: Query<&mut Window, With<PrimaryWindow>>, keys: Res<Input<KeyCode>>, keybinds: Res<Keybinds>) {
    if let Ok(mut window) = query.get_single_mut() {
        if window.focused {
            if keys.just_pressed(keybinds.toggle_mouse_grab) {
                let should_grab = window.cursor.grab_mode == CursorGrabMode::None;
                grab_window(&mut window, should_grab);
            }
        } else if window.cursor.grab_mode != CursorGrabMode::None {
            grab_window(&mut window, false);
        }
    } else {
        log::error!("No window found for focus grab");
    }
}

/// Grabs or un-grabs the mouse
///
/// # Arguments
/// * `window` - the window to grab or un-grab
/// * `should_grab` - `true` if the window should be grabbed, otherwise `false`
///
fn grab_window(window: &mut Window, should_grab: bool) {
    if should_grab {
        if window.cursor.grab_mode == CursorGrabMode::None {
            window.cursor.grab_mode = CursorGrabMode::Confined;
            window.cursor.visible = false;
        }
    } else if window.cursor.grab_mode != CursorGrabMode::None {
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
    }
}

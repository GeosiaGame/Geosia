//! UI Views for the game.

use std::collections::BTreeMap;

use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_egui::{
    EguiContext, EguiGlobalSettings, EguiStartupSet, egui,
    input::{EguiContextPointerPosition, EguiInputEvent},
};

use crate::prelude::*;

pub mod chat;

/// Registers the common UI types and systems.
pub fn common_game_ui_plugin(app: &mut App) {
    app.insert_resource(IsCursorGrabbed(false));
    app.add_observer(update_grab_mode);
    app.add_systems(Startup, setup_egui_theme.after(EguiStartupSet::InitContexts));
    app.add_systems(Last, center_cursor_delay);
}

/// True when the cursor is grabbed and not available to UIs.
#[derive(Resource, Deref, DerefMut)]
pub struct IsCursorGrabbed(pub bool);

/// Observer event for requesting a cursor grab mode change.
#[derive(Event, Deref, DerefMut)]
pub struct SetGrabMode(pub bool);

#[derive(Component)]
struct CenterCursorDelay(i32);

fn setup_egui_theme(egui_contexts: Query<&mut EguiContext>) {
    use egui::FontFamily::*;
    use egui::FontId;
    use egui::TextStyle;
    for mut ctx in egui_contexts {
        let ctx = ctx.get_mut();
        let fonts = egui::FontDefinitions::default();
        ctx.set_fonts(fonts);
        let text_styles: BTreeMap<_, _> = [
            (TextStyle::Heading, FontId::new(30.0, Proportional)),
            (TextStyle::Name("Heading2".into()), FontId::new(25.0, Proportional)),
            (TextStyle::Name("Context".into()), FontId::new(23.0, Proportional)),
            (TextStyle::Body, FontId::new(16.0, Proportional)),
            (TextStyle::Monospace, FontId::new(16.0, Proportional)),
            (TextStyle::Button, FontId::new(16.0, Proportional)),
            (TextStyle::Small, FontId::new(12.0, Proportional)),
        ]
        .into();
        ctx.all_styles_mut(|style| {
            style.text_styles = text_styles.clone();
        });
    }
}

fn update_grab_mode(
    trigger: Trigger<SetGrabMode>,
    mut window_q: Query<&mut Window, With<PrimaryWindow>>,
    mut state: ResMut<IsCursorGrabbed>,
    existing_delay: Query<Entity, With<CenterCursorDelay>>,
    mut egui_settings: ResMut<EguiGlobalSettings>,
    mut egui_contexts: Query<(Entity, &mut EguiContextPointerPosition), (With<EguiContext>, With<Window>)>,
    mut egui_input_event_writer: EventWriter<EguiInputEvent>,
    mut commands: Commands,
) {
    let Ok(mut window) = window_q.single_mut() else {
        return;
    };
    let window = &mut *window;
    let old_grabbed = window.cursor_options.grab_mode != CursorGrabMode::None;
    let new_grabbed = **trigger;
    if old_grabbed == new_grabbed {
        return;
    }
    existing_delay.iter().for_each(|e| commands.entity(e).despawn());
    if new_grabbed {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
        commands.spawn(CenterCursorDelay(2));
        state.0 = true;

        let dummy_position = bevy_egui::egui::pos2(-f32::INFINITY, -f32::INFINITY);
        for (entity, mut ctx) in egui_contexts.iter_mut() {
            ctx.position = dummy_position;
            egui_input_event_writer.write(EguiInputEvent {
                context: entity,
                event: bevy_egui::egui::Event::PointerMoved(dummy_position),
            });
        }
    } else {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
        state.0 = false;
    }
    egui_settings
        .input_system_settings
        .run_write_keyboard_input_events_system = !new_grabbed;
    egui_settings.input_system_settings.run_write_mouse_wheel_events_system = !new_grabbed;
    egui_settings.input_system_settings.run_write_ime_events_system = !new_grabbed;
    egui_settings
        .input_system_settings
        .run_write_window_pointer_moved_events_system = !new_grabbed;
    egui_settings.input_system_settings.run_write_window_touch_events_system = !new_grabbed;
}

fn center_cursor_delay(
    mut window_q: Query<&mut Window, With<PrimaryWindow>>,
    mut delay_q: Populated<(Entity, &mut CenterCursorDelay)>,
    mut commands: Commands,
) {
    let Ok(mut window) = window_q.single_mut() else {
        return;
    };
    let mut do_center = false;
    for (_, mut delay) in delay_q.iter_mut() {
        delay.0 -= 1;
        if delay.0 == 0 {
            do_center = true;
        }
    }
    if !do_center {
        return;
    }
    let window = &mut *window;
    let grabbed = window.cursor_options.grab_mode != CursorGrabMode::None;
    if grabbed {
        window.set_physical_cursor_position(Some(window.physical_size().as_dvec2() / 2.0));
    }
    delay_q.iter().for_each(|(e, _)| commands.entity(e).despawn());
}

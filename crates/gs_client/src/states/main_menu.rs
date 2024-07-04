//! The main menu state that lets the user start a single player game or connect to a server.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiContexts;
use gs_common::GAME_BRAND_NAME;

use crate::states::loading_game::LoadingTransitionParams;
use crate::states::{ClientAppState, MainMenuSystemSet};

/// The "plugin" implementing the main menu in the game.
pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (main_menu_ui,).in_set(MainMenuSystemSet));
    }
}

fn main_menu_ui(
    mut contexts: EguiContexts,
    mut quit: EventWriter<AppExit>,
    mut loading_data: ResMut<LoadingTransitionParams>,
    mut state_switch: ResMut<NextState<ClientAppState>>,
) {
    egui::Window::new(GAME_BRAND_NAME)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, (0.0, 0.0))
        .show(contexts.ctx_mut(), |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                if ui.button("Play singleplayer").clicked() {
                    *loading_data = LoadingTransitionParams::SinglePlayer {};
                    state_switch.set(ClientAppState::LoadingGame);
                }
                ui.add_space(8.0);
                if ui.button("Quit").clicked() {
                    quit.send(AppExit::Success);
                }
                ui.add_space(16.0);
            });
        });
}

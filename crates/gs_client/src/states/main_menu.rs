//! The main menu state that lets the user start a single player game or connect to a server.

use bevy_egui::EguiContextPass;
use bevy_egui::EguiContexts;
use bevy_egui::egui;
use gs_common::GAME_BRAND_NAME;
use gs_schemas::savefile::{SavefileMetadata, list_saves, new_save, saves_directory};

use crate::prelude::*;
use crate::states::loading_game::LoadingTransitionParams;
use crate::states::{ClientAppState, MainMenuSystemSet};

/// The "plugin" implementing the main menu in the game.
pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiContextPass, (main_menu_ui,).in_set(MainMenuSystemSet));
    }
}

struct MenuInputs {
    new_save_name: String,
    server_ip: String,
}

impl Default for MenuInputs {
    fn default() -> Self {
        Self {
            server_ip: String::from("[::1]:28032"),
            new_save_name: String::from("New Geosia Game"),
        }
    }
}

fn main_menu_ui(
    mut contexts: EguiContexts,
    mut quit: EventWriter<AppExit>,
    mut loading_data: ResMut<LoadingTransitionParams>,
    mut state_switch: ResMut<NextState<ClientAppState>>,
    mut menu_inputs: Local<MenuInputs>,
    mut saves: Local<Option<Vec<SavefileMetadata>>>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let metadata = saves.get_or_insert_with(|| {
        let (mut saves, errs) = list_saves(saves_directory());
        let errs = errs.into_result();
        if let Err(errs) = errs {
            warn!("Detected some issues while scanning for savefiles: {errs}");
        }
        saves.sort_by_key(|s| s.modified_at);
        saves.reverse();
        saves
    });
    egui::Window::new(GAME_BRAND_NAME)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, (0.0, 0.0))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading("Singleplayer");
                for save in metadata.iter() {
                    ui.separator();
                    if ui.button(&save.name).clicked() {
                        *loading_data = LoadingTransitionParams::SinglePlayer {
                            savefile_metadata: save.clone(),
                        };
                        state_switch.set(ClientAppState::LoadingGame);
                    }
                    ui.small(format!(
                        "Modified at {}\nCreated at {}\nDisk size {}",
                        save.modified_at, save.created_at, save.disk_size
                    ));
                }
                if metadata.is_empty() {
                    ui.separator();
                    ui.label("No saves found");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("New save:");
                    ui.text_edit_singleline(&mut menu_inputs.new_save_name);
                    if ui.button("Create and play").clicked() {
                        let savefile_metadata =
                            new_save(saves_directory(), &menu_inputs.new_save_name).expect("Could not create save");
                        *loading_data = LoadingTransitionParams::SinglePlayer { savefile_metadata };
                        state_switch.set(ClientAppState::LoadingGame);
                    }
                });
                ui.separator();
                ui.heading("Multiplayer");
                ui.label("Server IP");
                ui.text_edit_singleline(&mut menu_inputs.server_ip);
                if ui.button("Join multiplayer session").clicked() {
                    *loading_data = LoadingTransitionParams::MultiPlayer {
                        server_address_raw: menu_inputs.server_ip.clone(),
                    };
                    state_switch.set(ClientAppState::LoadingGame);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    quit.write(AppExit::Success);
                }
                ui.add_space(16.0);
            });
        });
}

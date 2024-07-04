//! Game state and state transition implementations

use bevy::prelude::{States, SystemSet};

pub mod in_game;
pub mod loading_game;
pub mod main_menu;

/// The main bevy state enum, controlling e.g. whether we are in game or in the main menu.
#[derive(States, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ClientAppState {
    /// The main menu for the game.
    #[default]
    MainMenu,
    /// The transition state that sets up the internal game state while streaming bootstrap data from the network/IPC.
    LoadingGame,
    /// The actual game, everything needed to run it is fully loaded at this point.
    InGame,
}

/// The tag for systems that should run in the main menu.
#[derive(SystemSet, Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MainMenuSystemSet;

/// The tag for systems that should run while loading a game.
#[derive(SystemSet, Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LoadingGameSystemSet;

pub use gs_common::InGameSystemSet;

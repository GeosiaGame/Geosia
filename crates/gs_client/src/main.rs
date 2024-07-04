#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]
#![allow(clippy::type_complexity)]

//! The clientside of Geosia - the main binary

use bevy::app::App;
use bevy::log::LogPlugin;
use gs_client::client_main;

fn main() {
    // Set up bevy's logging once per process
    App::new().add_plugins(LogPlugin::default()).run();
    client_main()
}

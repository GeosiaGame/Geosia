use bevy::app::App;
use bevy::log::LogPlugin;
use clap::Parser;

#[derive(Parser)]
#[command(name = "ocg_dedi_server", about = "OpenCubeGame dedicated server")]
struct CliOptions {}

fn main() {
    // Set up bevy's logging once per process
    App::new().add_plugins(LogPlugin::default()).run();
    let _cli = CliOptions::parse();
}

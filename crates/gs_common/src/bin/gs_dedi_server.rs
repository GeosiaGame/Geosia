//! Dedicated server binary for the game.
use eyre::Result;
use gs_common::dedicated_server::run_dedicated_server;
use gs_common::geosia_pre_main;

fn main() -> Result<()> {
    geosia_pre_main();
    run_dedicated_server()
}

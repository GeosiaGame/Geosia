//! The dedicated server main() implementation

use bevy::prelude::*;
use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::config::{GameConfig, ServerConfig};
use crate::prelude::*;
use crate::GameServer;

#[derive(Parser)]
#[command(name = "gs_dedi_server", about = "Geosia dedicated server")]
struct CliOptions {}

/// Starts the dedicated server CLI
pub fn run_dedicated_server() -> Result<()> {
    let _cli = CliOptions::parse();

    let game_config = GameConfig {
        server: ServerConfig {
            server_title: String::from("Dedicated server"),
            ..Default::default()
        },
    };
    let game_config = GameConfig::new_handle(game_config);
    let integ_server = GameServer::new(game_config).expect("Could not start dedicated server");
    integ_server.set_paused(false);

    if let Ok(mut rl) = DefaultEditor::new() {
        loop {
            match rl.readline("Geosia> ") {
                Ok(line) => {
                    let cmd = line.split_whitespace().next().unwrap_or("");
                    match cmd {
                        "" => {
                            continue;
                        }
                        "quit" | "stop" | "exit" => {
                            info!("Sending a shutdown command to the server...");
                            integ_server.shutdown().blocking_wait()?;
                            break;
                        }
                        _ => {
                            error!("Unknown command {cmd}");
                        }
                    }
                }
                Err(ReadlineError::Eof) => {
                    info!("stdin EOF reached");
                    break;
                }
                Err(ReadlineError::Interrupted) => {
                    info!("Interrupt signal received");
                    integ_server.shutdown().blocking_wait()?;
                    break;
                }
                Err(ReadlineError::WindowResized) => continue,
                Err(e) => {
                    error!("Error reading commandline prompt: {e}");
                    break;
                }
            }
        }
    }

    Ok(())
}

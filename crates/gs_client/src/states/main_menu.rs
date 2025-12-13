//! The main menu state that lets the user start a single player game or connect to a server.

use std::fmt::Write;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::time::Instant;

use bevy_egui::EguiContextPass;
use bevy_egui::EguiContexts;
use bevy_egui::egui;
use gs_common::GAME_BRAND_NAME;
use gs_common::network::PeerAddress;
use gs_common::network::transport::{
    NetworkConnection, PacketWrapper, RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS, quinn_client_config,
};
use gs_schemas::GameSide;
use gs_schemas::savefile::{SavefileMetadata, list_saves, new_save, saves_directory};
use gs_schemas::schemas::network_capnp::PacketId;
use gs_schemas::schemas::new_simple_packet_builder;
use quinn::{Endpoint, EndpointConfig};
use socket2::{Domain, Socket};
use tokio::task::LocalSet;

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
    server_status: AsyncWatchReceiver<String>,
    server_status_tx: AsyncWatchSender<String>,
}

impl Default for MenuInputs {
    fn default() -> Self {
        let (tx, rx) = async_watch_channel(String::new());
        Self {
            server_ip: String::from("[::1]:28032"),
            new_save_name: String::from("New Geosia Game"),
            server_status: rx,
            server_status_tx: tx,
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
                ui.label(menu_inputs.server_status.borrow().as_str());
                if ui.button("Join multiplayer session").clicked() {
                    *loading_data = LoadingTransitionParams::MultiPlayer {
                        server_address_raw: menu_inputs.server_ip.clone(),
                    };
                    state_switch.set(ClientAppState::LoadingGame);
                }
                if ui.button("Query server IP").clicked() {
                    let _ = menu_inputs.server_status_tx.send(String::from("Querying..."));
                    let ip = menu_inputs.server_ip.clone();
                    let writer = menu_inputs.server_status_tx.clone();
                    std::thread::spawn(move || server_query(ip, writer));
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    quit.write(AppExit::Success);
                }
                ui.add_space(16.0);
            });
        });
}

fn server_query(ip: String, result_writer: AsyncWatchSender<String>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build();
    let rt = match rt {
        Ok(rt) => rt,
        Err(err) => {
            let _ = result_writer.send(err.to_string());
            return;
        }
    };
    let local_set = LocalSet::new();
    let result = local_set.block_on(&rt, server_query_task(ip));
    match result {
        Ok(desc) => {
            let _ = result_writer.send(desc);
        }
        Err(err) => {
            let _ = result_writer.send(format!("Error: {err:?}"));
        }
    }
}

async fn server_query_task(ip: String) -> Result<String> {
    let remote_address = ip.parse()?;
    let bind_addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0);
    let socket = Socket::new(Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
    socket.set_only_v6(false)?;
    socket.bind(&bind_addr.into())?;
    let local_addr = socket.local_addr()?;
    let endpoint = Endpoint::new(
        EndpointConfig::default(),
        None,
        socket.into(),
        quinn::default_runtime().unwrap(),
    )?;
    let quic_connection = endpoint
        .connect_with(quinn_client_config(), remote_address, "example.com")?
        .await?;
    let address = PeerAddress::Network {
        local: local_addr.as_socket().context("Obtaining local socket address")?,
        remote: quic_connection.remote_address(),
    };
    let net_conn = NetworkConnection::wrap_remote(GameSide::Client, address, endpoint, quic_connection);
    let stream = net_conn.open_stream().await?;
    let mut pkt = new_simple_packet_builder();
    {
        let mut root = pkt.init_root();
        root.set_id(PacketId::GetServerMetadata);
        root.set_timestamp_ms(0);
        root.set_simple_payload(1);
    }
    let pkt = PacketWrapper::from(pkt);
    let send_time = Instant::now();
    stream.send_packet(pkt).context("Sending metadata request")?;
    let response = stream.recv_packet().await.context("Receiving metadata request")?;
    let recv_time = Instant::now();
    stream.close();
    drop(stream);
    tokio::task::spawn_local(async move { net_conn.close().await });
    let response = response.parse_typed::<gs_schemas::schemas::network_capnp::game_server_metadata::Owned>(
        RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS,
    )?;
    let response = response.get()?;
    if response.get_id()? != PacketId::GetServerMetadata {
        return Err(anyhow::anyhow!(
            "Invalid packet ID received, got {} but expected GetServerMetadata",
            response.get_id()?
        ));
    }
    let mut output = String::new();
    writeln!(
        &mut output,
        "{ip} responded in {dur:.1} ms",
        dur = recv_time.duration_since(send_time).as_micros() as f64 / 1000.0
    )?;
    let info = response.get_payload()?;
    writeln!(
        &mut output,
        "{}/{} players",
        info.get_player_count(),
        info.get_player_limit()
    )?;
    writeln!(&mut output, "Title: {}", info.get_title()?.to_str()?)?;
    Ok(output)
}

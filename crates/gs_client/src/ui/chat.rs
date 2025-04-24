//! The chat HUD overlay and typing window.

use std::collections::VecDeque;

use bevy_egui::EguiContextPass;
use bevy_egui::EguiContexts;
use bevy_egui::egui;
use gs_common::InGameSystemSet;
use gs_common::network::transport::PacketWrapper;
use gs_schemas::dependencies::kstring::KString;
use gs_schemas::schemas::new_packet_builder;

use crate::network::AuthenticatedNetworkClient;
use crate::network::client_packet_handlers::ChatMessage;
use crate::prelude::*;

/// The bevy [`Plugin`] for displaying the in-game chat and handling input.
pub fn chat_plugin(app: &mut App) {
    app.insert_resource(ChatState {
        messages: VecDeque::with_capacity(MAX_MESSAGES + 2),
        predicted_messages: VecDeque::with_capacity(2),
        entry_string: String::with_capacity(128),
    });
    app.add_observer(chat_message_observer);

    app.add_systems(
        EguiContextPass,
        (chat_ui)
            .in_set(InGameSystemSet)
            .run_if(resource_exists::<AuthenticatedNetworkClient>),
    );
}

#[derive(Resource)]
struct ChatState {
    messages: VecDeque<KString>,
    predicted_messages: VecDeque<KString>,
    entry_string: String,
}

const MAX_MESSAGES: usize = 256;

fn chat_message_observer(trigger: Trigger<ChatMessage>, mut state: ResMut<ChatState>) {
    let state = &mut *state;
    // reuse the buffer of the predicted message
    if trigger.is_echo {
        state.predicted_messages.pop_front().unwrap_or_default();
    }
    state.messages.push_back(trigger.message.clone());
    if state.messages.len() > MAX_MESSAGES {
        let to_remove = state.messages.len() - MAX_MESSAGES;
        state.messages.drain(0..to_remove);
    }
}

fn chat_ui(mut ui: EguiContexts, mut state: ResMut<ChatState>, client: Res<AuthenticatedNetworkClient>) {
    let state = &mut *state;
    let Some(ctx) = ui.try_ctx_mut() else {
        return;
    };
    let screen = ctx.screen_rect();
    let client = &*client;
    let chat_bounds = egui::Rect::from_min_size(
        screen.left_center() + egui::vec2(8.0, 0.0),
        egui::vec2(screen.width().max(800.0) * 0.33, screen.height() * 0.4),
    );
    egui::Window::new("Chat")
        .default_rect(chat_bounds)
        .fixed_rect(chat_bounds)
        .scroll(egui::Vec2b::new(false, true))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .frame(egui::Frame::new())
        .title_bar(false)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.with_layout(
                egui::Layout::top_down(egui::Align::Min).with_main_align(egui::Align::Max),
                |ui| {
                    for msg in state.messages.iter() {
                        ui.add(egui::Label::new(
                            egui::RichText::new(msg.as_str()).color(egui::Rgba::WHITE),
                        ));
                        ui.add_space(2.0);
                    }
                    for msg in state.predicted_messages.iter() {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Spinner::new()
                                    .size(ui.style().text_styles.get(&egui::TextStyle::Body).unwrap().size),
                            );
                            ui.add(egui::Label::new(
                                egui::RichText::new(msg.as_str()).color(egui::Rgba::from_white_alpha(0.8)),
                            ));
                        });
                        ui.add_space(2.0);
                    }
                    let edit_resp =
                        ui.add(egui::TextEdit::singleline(&mut state.entry_string).desired_width(f32::INFINITY));
                    if edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let mut packet = new_packet_builder::<capnp::text::Owned>();
                        let mut root = packet.init_root();
                        root.set_id(rpc::PacketId::ChatMessage);
                        root.set_timestamp_ms(client.packet_timestamp());
                        if let Err(e) = root.set_payload(&state.entry_string) {
                            error!("Could not send chat message \"{}\": {}", state.entry_string, e);
                            return;
                        }
                        let packet = PacketWrapper::from(packet);
                        let _ = client.main_c2s_stream.send_packet(packet);
                        state
                            .predicted_messages
                            .push_back(KString::from_string(format!("<...> {}", state.entry_string)));
                        state.entry_string.clear();
                        edit_resp.scroll_to_me(None);
                    }
                },
            );
        });
}

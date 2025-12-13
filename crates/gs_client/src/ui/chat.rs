//! The chat HUD overlay and typing window.

use std::collections::VecDeque;

use bevy_egui::egui;
use bevy_egui::input::egui_wants_any_keyboard_input;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use gs_common::InGameSystemSet;
use gs_common::network::transport::PacketWrapper;
use gs_schemas::dependencies::kstring::KString;
use gs_schemas::schemas::new_packet_builder;

use super::IsCursorGrabbed;
use super::SetGrabMode;
use crate::debugcam::KeyBindings;
use crate::network::AuthenticatedNetworkClient;
use crate::network::client_packet_handlers::ChatMessage;
use crate::prelude::*;

/// The bevy [`Plugin`] for displaying the in-game chat and handling input.
pub fn chat_plugin(app: &mut App) {
    app.insert_resource(ChatState {
        messages: VecDeque::with_capacity(MAX_MESSAGES + 2),
        predicted_messages: VecDeque::with_capacity(2),
        entry_string: String::with_capacity(128),
        text_was_focused: false,
        request_edit_focus: false,
    });
    app.add_observer(chat_message_observer);

    app.add_systems(
        EguiPrimaryContextPass,
        (open_chat.run_if(not(egui_wants_any_keyboard_input)), chat_ui)
            .chain()
            .in_set(InGameSystemSet)
            .run_if(resource_exists::<AuthenticatedNetworkClient>),
    );
}

#[derive(Resource)]
struct ChatState {
    messages: VecDeque<KString>,
    predicted_messages: VecDeque<KString>,
    entry_string: String,
    text_was_focused: bool,
    request_edit_focus: bool,
}

const MAX_MESSAGES: usize = 256;

fn chat_message_observer(trigger: On<ChatMessage>, mut state: ResMut<ChatState>) {
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

fn open_chat(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<ChatState>, keybinds: Res<KeyBindings>) {
    if keys.just_released(keybinds.open_chat) {
        state.request_edit_focus = true;
    }
}

fn chat_ui(
    mut ui: EguiContexts,
    mut state: ResMut<ChatState>,
    client: Res<AuthenticatedNetworkClient>,
    cursor_grabbed: Res<IsCursorGrabbed>,
    mut commands: Commands,
) {
    let state = &mut *state;
    let Ok(ctx) = ui.ctx_mut() else {
        return;
    };
    let screen = ctx.content_rect();
    let client = &*client;
    let chat_bounds = egui::Rect::from_min_size(
        screen.left_center() + egui::vec2(8.0, 0.0),
        egui::vec2(screen.width().max(800.0) * 0.33, screen.height() * 0.4),
    );
    egui::Window::new("Chat")
        .default_rect(chat_bounds)
        .fixed_rect(chat_bounds)
        .frame(egui::Frame::new())
        .title_bar(false)
        .collapsible(false)
        .interactable(!**cursor_grabbed)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                let edit_resp = ui.add_visible(
                    state.text_was_focused,
                    egui::TextEdit::singleline(&mut state.entry_string)
                        .desired_width(f32::INFINITY)
                        .background_color(if state.text_was_focused {
                            ui.visuals().extreme_bg_color
                        } else {
                            egui::Color32::from_black_alpha(0)
                        }),
                );
                if state.request_edit_focus {
                    state.request_edit_focus = false;
                    state.text_was_focused = true;
                    edit_resp.request_focus();
                    commands.trigger(SetGrabMode(false));
                }
                state.text_was_focused = edit_resp.has_focus();
                if edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let mut packet = new_packet_builder::<capnp::text::Owned>();
                    let mut root = packet.init_root();
                    root.set_id(rpc::PacketId::ChatMessage);
                    root.set_timestamp_ms(client.packet_timestamp());
                    if let Err(e) = root.set_payload(&state.entry_string) {
                        error!("Could not send chat message \"{}\": {}", state.entry_string, e);
                    } else {
                        let packet = PacketWrapper::from(packet);
                        let _ = client.main_c2s_stream.send_packet(packet);
                        state
                            .predicted_messages
                            .push_back(KString::from_string(format!("<...> {}", state.entry_string)));
                        state.entry_string.clear();
                        edit_resp.scroll_to_me(None);
                        commands.trigger(SetGrabMode(true));
                    }
                }
                if edit_resp.lost_focus() {
                    state.entry_string.clear();
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            for msg in state.messages.iter() {
                                ui.add(egui::Label::new(
                                    egui::RichText::new(msg.as_str())
                                        .color(egui::Rgba::WHITE)
                                        .background_color(egui::Rgba::from_black_alpha(0.4)),
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
                        });
                    });
            });
        });
}

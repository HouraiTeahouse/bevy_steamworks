use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, FontSize};
use bevy::ui_widgets::{ControlOrientation, ScrollArea, Scrollbar, ScrollbarThumb};
use bevy_steamworks::networking_types::{NetworkingIdentity, SendFlags};
use bevy_steamworks::*;

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
enum AppState {
    #[default]
    Menu,
    Lobby,
}

/// Only present when in lobby.
#[derive(Resource)]
struct LobbyState {
    lobby_id: LobbyId,
}

#[derive(Component, Default, Clone)]
struct ChatLog;

#[derive(Component, Default, Clone)]
struct PlayerList;

#[derive(Component, Default, Clone)]
struct ChatInput;

#[derive(Message)]
struct ChatLine(String);

#[derive(Message)]
struct SendLobbyChat;

const MAX_LOBBY_MESSAGE_SIZE: usize = 4096;
const P2P_MESSAGE_BATCH_AMOUNT: usize = 100;

const BACKGROUND: Color = Color::srgb(0.12, 0.12, 0.12);
const PRIMARY: Color = Color::srgb(0.25, 0.25, 0.30); // button idle
const SECONDARY: Color = Color::srgb(0.18, 0.18, 0.18); // button hovered
const PRESSED: Color = Color::srgb(0.10, 0.10, 0.13); // button pressed
const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);

fn main() {
    App::new()
        .add_plugins(SteamworksPlugin::init_app(480).unwrap())
        .add_plugins(DefaultPlugins)
        .init_state::<AppState>()
        .add_message::<ChatLine>()
        .add_message::<SendLobbyChat>()
        .add_systems(Startup, setup)
        .add_systems(Update, handle_steam_events)
        .add_systems(OnEnter(AppState::Menu), menu.spawn())
        .add_systems(OnEnter(AppState::Lobby), setup_lobby)
        .add_systems(
            Update,
            (
                fill_player_list,
                receive_p2p_messages,
                send_lobby_chat_on_enter,
                send_lobby_chat,
                append_chat_lines,
            )
                .run_if(in_state(AppState::Lobby)),
        )
        .run();
}

fn setup(mut commands: Commands, client: Res<Client>) {
    commands.spawn(Camera2d);

    // Auto-accept incoming P2P networking sessions.
    let net = client.networking_messages();
    net.session_request_callback(|req| {
        req.accept();
    });
    net.session_failed_callback(|info| {
        eprintln!("[Net] Session failed: {info:#?}");
    });
}

/// Steam callbacks handling, forwarded by `bevy_steamworks` as a [`SteamworksEvent`] message.
fn handle_steam_events(
    mut reader: MessageReader<SteamworksEvent>,
    mut commands: Commands,
    client: Res<Client>,
    mut next_state: ResMut<NextState<AppState>>,
    mut chat: MessageWriter<ChatLine>,
    lobby: Option<Res<LobbyState>>,
    player_list: Query<Entity, With<PlayerList>>,
) {
    let mut current_lobby_id = lobby.map(|l| l.lobby_id);

    for SteamworksEvent::CallbackResult(cb) in reader.read() {
        match cb {
            // A "Join Game" request from the Steam friends list overlay.
            CallbackResult::GameLobbyJoinRequested(ev) if current_lobby_id.is_none() => {
                client
                    .matchmaking()
                    .join_lobby(ev.lobby_steam_id, |result| {
                        if result.is_err() {
                            eprintln!("[Lobby] Failed to join lobby");
                        }
                    });
            }

            // Fires for both hosting and joining.
            CallbackResult::LobbyEnter(ev)
                if ev.chat_room_enter_response == ChatRoomEnterResponse::Success
                    && current_lobby_id != Some(ev.lobby) =>
            {
                current_lobby_id = Some(ev.lobby);
                commands.insert_resource(LobbyState { lobby_id: ev.lobby });
                next_state.set(AppState::Lobby);
            }

            CallbackResult::LobbyChatMsg(ev) if current_lobby_id == Some(ev.lobby) => {
                let mut buf = vec![0u8; MAX_LOBBY_MESSAGE_SIZE];
                let data = client
                    .matchmaking()
                    .get_lobby_chat_entry(ev.lobby, ev.chat_id, &mut buf);
                if let Ok(text) = std::str::from_utf8(data) {
                    let name = client.friends().get_friend(ev.user).name();
                    chat.write(ChatLine(format!("[Lobby] {name}: {text}")));
                }
            }

            // A member joined/left the lobby: log it and rebuild the roster.
            CallbackResult::LobbyChatUpdate(ev) if current_lobby_id == Some(ev.lobby) => {
                let name = client.friends().get_friend(ev.user_changed).name();
                let action = match ev.member_state_change {
                    ChatMemberStateChange::Entered => "joined",
                    ChatMemberStateChange::Left => "left",
                    ChatMemberStateChange::Disconnected => "disconnected",
                    ChatMemberStateChange::Kicked => "was kicked",
                    ChatMemberStateChange::Banned => "was banned",
                };
                chat.write(ChatLine(format!("** {name} {action} **")));
                // Clear the roster; `fill_player_list` rebuilds it.
                if let Ok(container) = player_list.single() {
                    commands.entity(container).despawn_children();
                }
            }

            _ => {}
        }
    }
}

fn receive_p2p_messages(client: Res<Client>, mut chat: MessageWriter<ChatLine>) {
    let net = client.networking_messages();
    for message in net.receive_messages_on_channel(0, P2P_MESSAGE_BATCH_AMOUNT) {
        if let Ok(text) = std::str::from_utf8(message.data()) {
            let name = message
                .identity_peer()
                .steam_id()
                .map(|sid| client.friends().get_friend(sid).name())
                .unwrap_or_else(|| "Unknown".to_string());
            chat.write(ChatLine(format!("[Net] {name}: {text}")));
        }
    }
}

fn menu() -> impl Scene {
    bsn! {
        DespawnOnExit<AppState>(AppState::Menu)
        Node {
            width: percent(100.0),
            height: percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(20.0),
        }
        BackgroundColor(BACKGROUND)
        Children [
            (
                button_base()
                Node { padding: UiRect::axes(px(30.0), px(15.0)) }
                on(|_: On<Pointer<Click>>, client: Res<Client>| {
                    client.matchmaking().create_lobby(LobbyType::FriendsOnly, 4, |result| {
                        if let Err(err) = result {
                            eprintln!("[Lobby] Failed to create lobby: {err:?}");
                        }
                    });
                })
                Children [
                    label("Host Lobby", 28.0)
                ]
            ),
            (
                Text({ "Or join through the Steam friends list".to_string() })
                TextFont { font_size: FontSize::Px(18.0) }
                TextColor(TEXT_COLOR)
            ),
        ]
    }
}

fn setup_lobby(mut commands: Commands, lobby: Res<LobbyState>) {
    commands.spawn_scene(lobby_screen(lobby.lobby_id));
}

fn lobby_screen(lobby_id: LobbyId) -> impl Scene {
    bsn! {
        DespawnOnExit<AppState>(AppState::Lobby)
        Node {
            width: percent(100.0),
            height: percent(100.0),
            flex_direction: FlexDirection::Column,
        }
        BackgroundColor(BACKGROUND)
        Children [
            // Top bar: lobby id.
            (
                Node { padding: UiRect::all(px(10.0)) }
                Children [(
                    Text({ format!("Lobby: {}", lobby_id.raw()) })
                    TextFont { font_size: FontSize::Px(20.0) }
                    TextColor(TEXT_COLOR)
                )]
            ),
            // Main content: player list (left) + chat log (right).
            (
                Node {
                    width: percent(100.0),
                    flex_grow: 1.0,
                    // Take exactly the leftover height instead of growing to fit chat content,
                    // so the scroll viewport below can actually clip and scroll.
                    min_height: px(0.0),
                    flex_direction: FlexDirection::Row,
                }
                Children [
                    (
                        Node {
                            width: px(200.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(px(10.0)),
                            row_gap: px(4.0),
                        }
                        BackgroundColor(SECONDARY)
                        Children [
                            (
                                Text({ "Players:".to_string() })
                                TextFont { font_size: FontSize::Px(18.0) }
                                TextColor(TEXT_COLOR)
                            ),
                            (
                                PlayerList
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(2.0),
                                }
                            ),
                        ]
                    ),
                    // Chat area: scroll viewport + vertical scrollbar side by side.
                    (
                        Node {
                            flex_grow: 1.0,
                            min_height: px(0.0),
                            flex_direction: FlexDirection::Row,
                        }
                        Children [
                            (
                                #chat_viewport
                                ScrollArea
                                Node {
                                    flex_grow: 1.0,
                                    min_height: px(0.0),
                                    flex_direction: FlexDirection::Column,
                                    padding: UiRect::all(px(10.0)),
                                    overflow: Overflow::scroll_y(),
                                }
                                BackgroundColor(BACKGROUND)
                                Children [(
                                    ChatLog
                                    Node {
                                        flex_direction: FlexDirection::Column,
                                        row_gap: px(2.0),
                                    }
                                )]
                            ),
                            (
                                Scrollbar {
                                    target: #chat_viewport,
                                    orientation: ControlOrientation::Vertical,
                                    min_thumb_length: 24.0,
                                }
                                Node { width: px(10.0), height: percent(100.0) }
                                BackgroundColor(SECONDARY)
                                Children [(
                                    ScrollbarThumb
                                    BackgroundColor(PRIMARY)
                                )]
                            ),
                        ]
                    ),
                ]
            ),
            // Bottom bar: leave, text input, send buttons.
            (
                Node {
                    width: percent(100.0),
                    padding: UiRect::all(px(8.0)),
                    column_gap: px(8.0),
                    align_items: AlignItems::Center,
                }
                Children [
                    lobby_button("Leave", on(|_: On<Pointer<Click>>,
                       mut commands: Commands,
                       client: Res<Client>,
                       lobby: Res<LobbyState>,
                       mut next_state: ResMut<NextState<AppState>>| {
                        client.matchmaking().leave_lobby(lobby.lobby_id);
                        commands.remove_resource::<LobbyState>();
                        next_state.set(AppState::Menu);
                    })),
                    (
                        ChatInput
                        EditableText { allow_newlines: false }
                        AutoFocus
                        Node {
                            flex_grow: 1.0,
                            border: UiRect::all(px(2.0)),
                            padding: UiRect::axes(px(8.0), px(5.0)),
                        }
                        BorderColor { top: PRIMARY, right: PRIMARY, bottom: PRIMARY, left: PRIMARY }
                        BackgroundColor(BACKGROUND)
                        TextFont { font_size: FontSize::Px(18.0) }
                        TextColor(TEXT_COLOR)
                    ),
                    lobby_button("Lobby Chat", on(|_: On<Pointer<Click>>,
                       mut submit: MessageWriter<SendLobbyChat>| {
                        submit.write(SendLobbyChat);
                    })),
                    lobby_button("P2P Message", on(|_: On<Pointer<Click>>,
                       client: Res<Client>,
                       lobby: Res<LobbyState>,
                       mut chat: MessageWriter<ChatLine>,
                       mut input: Query<&mut EditableText, With<ChatInput>>| {
                        let Ok(mut editor) = input.single_mut() else {
                            return;
                        };
                        let text = editor.value().to_string();
                        let text = text.trim();
                        if text.is_empty() {
                            return;
                        }

                        let me = client.user().steam_id();
                        let net = client.networking_messages();
                        for member in client.matchmaking().lobby_members(lobby.lobby_id) {
                            if member != me {
                                let identity = NetworkingIdentity::new_steam_id(member);
                                let _ = net.send_message_to_user(
                                    identity,
                                    SendFlags::RELIABLE,
                                    text.as_bytes(),
                                    0,
                                );
                            }
                        }

                        // Echo locally.
                        let my_name = client.friends().name();
                        chat.write(ChatLine(format!("[Net] {my_name}: {text}")));
                        editor.clear();
                    })),
                ]
            ),
        ]
    }
}

/// Shared button base styling.
fn button_base() -> impl Scene {
    bsn! {
        BackgroundColor(PRIMARY)
        on(|ev: On<Pointer<Over>>, mut colors: Query<&mut BackgroundColor>| {
            if let Ok(mut bg) = colors.get_mut(ev.entity) {
                bg.0 = SECONDARY;
            }
        })
        on(|ev: On<Pointer<Out>>, mut colors: Query<&mut BackgroundColor>| {
            if let Ok(mut bg) = colors.get_mut(ev.entity) {
                bg.0 = PRIMARY;
            }
        })
        on(|ev: On<Pointer<Press>>, mut colors: Query<&mut BackgroundColor>| {
            if let Ok(mut bg) = colors.get_mut(ev.entity) {
                bg.0 = PRESSED;
            }
        })
        on(|ev: On<Pointer<Release>>, mut colors: Query<&mut BackgroundColor>| {
            if let Ok(mut bg) = colors.get_mut(ev.entity) {
                bg.0 = SECONDARY;
            }
        })
    }
}

/// A lobby-bar button: [`button_base`] plus a click observer and a label.
fn lobby_button(text: &'static str, on_click: impl Scene) -> impl Scene {
    bsn! {
        button_base()
        Node { padding: UiRect::axes(px(16.0), px(8.0)) }
        {on_click}
        Children [ label(text, 16.0) ]
    }
}

/// Shared text styling.
fn label(text: &'static str, size: f32) -> impl Scene {
    bsn! {
        Text({ text.to_string() })
        TextFont { font_size: FontSize::Px(size) }
        TextColor(TEXT_COLOR)
    }
}

/// Shared text row.
fn text_row(text: impl Into<String>) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(16.),
            ..default()
        },
        TextColor(TEXT_COLOR),
    )
}

fn send_lobby_chat_on_enter(
    keys: Res<ButtonInput<KeyCode>>,
    mut submit: MessageWriter<SendLobbyChat>,
) {
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        submit.write(SendLobbyChat);
    }
}

fn send_lobby_chat(
    mut reader: MessageReader<SendLobbyChat>,
    client: Res<Client>,
    lobby: Res<LobbyState>,
    mut input: Query<&mut EditableText, With<ChatInput>>,
) {
    for _ in reader.read() {
        let Ok(mut editor) = input.single_mut() else {
            continue;
        };
        let text = editor.value().to_string();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let _ = client
            .matchmaking()
            .send_lobby_chat_message(lobby.lobby_id, text.as_bytes());
        editor.clear();
    }
}

fn append_chat_lines(
    mut reader: MessageReader<ChatLine>,
    mut commands: Commands,
    chat_log: Query<Entity, With<ChatLog>>,
) {
    let Ok(container) = chat_log.single() else {
        return;
    };
    for ChatLine(msg) in reader.read() {
        commands.entity(container).with_child(text_row(msg.clone()));
    }
}

/// Fills the roster from the current lobby membership whenever the list is empty.
fn fill_player_list(
    mut commands: Commands,
    client: Res<Client>,
    lobby: Res<LobbyState>,
    player_list: Query<Entity, (With<PlayerList>, Without<Children>)>,
) {
    let Ok(container) = player_list.single() else {
        return;
    };

    let me = client.user().steam_id();
    for member in client.matchmaking().lobby_members(lobby.lobby_id) {
        let name = client.friends().get_friend(member).name();
        let label = if member == me {
            format!("{name} (you)")
        } else {
            name
        };
        commands.entity(container).with_child(text_row(label));
    }
}

//! This crate provides a [Bevy](https://bevyengine.org/) plugin for integrating with
//! the Steamworks SDK.
//!
//! ## Installation
//! Add the following to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! bevy-steamworks = "0.1.0"
//! ```
//!
//! Ensure that your computer has all the needed
//! [requirements](https://rust-lang.github.io/rust-bindgen/requirements.html) to use
//! [bindgen](https://github.com/rust-lang/rust-bindgen).
//!
//! Download and install the [steamworks sdk](https://partner.steamgames.com/doc/sdk)
//! and set the environment variable `STEAM_SDK_LOCATION` to point to it.
//!
//! At runtime, a "steam_appid.txt" file with the registered Steam App ID of the game
//! is required in the same directory as the executable.
//!
//! ## Usage
//!
//! To add the plugin to your game, simply add the `SteamworksPlugin` to your
//! `App`.
//!
//! ```rust
//! use bevy::prelude::*;
//! use bevy_steamworks::SteamworksPlugin;
//!
//! fn main() {
//!   App::build()
//!       .add_plugins(DefaultPlugins)
//!       .add_plugin(SteamworksPlugin)
//!       .run()
//! }
//! ```
//!
//! The plugin adds `steamworks::Client` as a Bevy ECS resource, which can be
//! accessed like any other resource in Bevy. The client implements `Send` and `Sync`
//! and can be used to make requests via the SDK from any of Bevy's threads. However,
//! any asynchronous callbacks from Steam will only run on the main thread.
//!
//! The plugin will automatically call `SingleClient::run_callbacks` on the Bevy
//! main thread every frame, so there is no need to run it manually.
//!
//! **NOTE**: If the plugin fails to initialize (i.e. `Client::init()` fails and
//! returns an error, an error wil lbe logged (via `bevy_log`), but it will not
//! panic. In this case, it may be necessary to use `Option<Res<Client>>` instead.
//!
//! ```rust
//! use bevy_steamworks::{Client, FriendFlags};
//!
//! fn steam_system(steam_client: Res<Client>) {
//!   for friend in client.friends().get_friends(FriendFlags::IMMEDIATE) {
//!     println!("Friend: {:?} - {}({:?})", friend.id(), friend.name(), friend.state());
//!   }
//! }
//!
//! fn main() {
//!   App::build()
//!       .add_plugins(DefaultPlugins)
//!       .add_plugin(SteamworksPlugin)
//!       .add_startup_system(steam_system.system())
//!       .run()
//! }
//! ```

use bevy_app::{App, EventWriter, Plugin};
use bevy_ecs::{schedule::*, system::*};
use bevy_log::error;
use std::sync::{Arc, Mutex};
pub use steamworks::*;

struct SteamEvents<T> {
    _callback: CallbackHandle,
    pending: Arc<Mutex<Vec<T>>>,
}

pub struct SteamworksPlugin;

impl Plugin for SteamworksPlugin {
    fn build(&self, app: &mut App) {
        if app.world.contains_resource::<Client<ClientManager>>() {
            return;
        }
        match Client::init() {
            Err(err) => error!("Failed to initialize Steamworks client: {}", err),
            Ok((client, single)) => {
                app.insert_resource(client.clone())
                    .insert_non_send_resource(single)
                    .add_system(run_steam_callbacks.system().label(STEAM_CALLBACKS));

                add_event::<AuthSessionTicketResponse>(app, &client);
                add_event::<GameLobbyJoinRequested>(app, &client);
                add_event::<P2PSessionConnectFail>(app, &client);
                add_event::<P2PSessionRequest>(app, &client);
                add_event::<PersonaStateChange>(app, &client);
                add_event::<UserAchievementStored>(app, &client);
                add_event::<UserStatsReceived>(app, &client);
                add_event::<UserStatsStored>(app, &client);
                add_event::<ValidateAuthTicketResponse>(app, &client);
            }
        }
    }
}

const STEAM_CALLBACKS: &str = "run_steam_callbacks";

fn run_steam_callbacks(client: NonSend<SingleClient>) {
    client.run_callbacks();
}

fn flush_events<T: Send + Sync + 'static>(
    events: ResMut<SteamEvents<T>>,
    mut output: EventWriter<T>,
) {
    let mut pending = events.pending.lock().unwrap();
    output.send_batch(pending.drain(0..));
}

fn add_event<T: Callback + Send + Sync + 'static>(
    app: &mut App,
    client: &Client<ClientManager>,
) {
    let pending = Arc::new(Mutex::new(Vec::new()));
    let pending_in = pending.clone();
    let callback = client.register_callback::<T, _>(move |evt| {
        pending_in.lock().unwrap().push(evt);
    });
    let events = SteamEvents::<T> {
        _callback: callback,
        pending,
    };
    app.add_event::<T>()
        .insert_resource(events)
        .add_system(flush_events::<T>.system().after(STEAM_CALLBACKS));
}

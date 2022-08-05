#![deny(missing_docs)]

//! This crate provides a [Bevy](https://bevyengine.org/) plugin for integrating with
//! the Steamworks SDK.
//!
//! ## Bevy Version Supported
//!
//! |Bevy Version |bevy\_steamworks|
//! |:------------|:---------------|
//! |git (main)   |git (develop)   |
//! |0.8          |0.5             |
//! |0.7          |0.4             |
//! |0.6          |0.2, 0.3        |
//! |0.5          |0.1             |
//!
//! ## Installation
//! Add the following to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! bevy-steamworks = "0.5"
//! ```
//!
//! Ensure that your build environment has all the needed
//! [requirements](https://rust-lang.github.io/rust-bindgen/requirements.html) to use
//! [bindgen](https://github.com/rust-lang/rust-bindgen).
//!
//! Download and install the [steamworks sdk](https://partner.steamgames.com/doc/sdk)
//! and set the environment variable `STEAM_SDK_LOCATION` to point to it.
//!
//! ## Usage
//!
//! To add the plugin to your app, simply add the `SteamworksPlugin` to your
//! `App`. This will require the `AppId` provided to you by Valve for initialization.
//!
//! ```rust no_run
//! use bevy::prelude::*;
//! use bevy_steamworks::*;
//!
//! fn main() {
//!   // Use the demo Steam AppId for SpaceWar
//!   App::new()
//!       .add_plugins(DefaultPlugins)
//!       .add_plugin(SteamworksPlugin::new(AppId(480)))
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
//! ```rust no_run
//! use bevy::prelude::*;
//! use bevy_steamworks::*;
//!
//! fn steam_system(steam_client: Res<Client>) {
//!   for friend in steam_client.friends().get_friends(FriendFlags::IMMEDIATE) {
//!     println!("Friend: {:?} - {}({:?})", friend.id(), friend.name(), friend.state());
//!   }
//! }
//!
//! fn main() {
//!   // Use the demo Steam AppId for SpaceWar
//!   App::new()
//!       .add_plugins(DefaultPlugins)
//!       .add_plugin(SteamworksPlugin::new(AppId(480)))
//!       .add_startup_system(steam_system)
//!       .run()
//! }
//! ```

use bevy_app::{App,  Plugin};
use bevy_ecs::{schedule::*, event::EventWriter, system::*};
use parking_lot::Mutex;
use std::sync::Arc;
pub use steamworks::*;

struct SteamEvents<T> {
    _callback: CallbackHandle,
    pending: Arc<Mutex<Vec<T>>>,
}

/// A Bevy [`Plugin`] for adding support for the Steam SDK.
///
/// [`Plugin`]: bevy_app::Plugin
pub struct SteamworksPlugin(AppId);

impl SteamworksPlugin {
    /// Creates a new `SteamworksPlugin`. The provided `app_id` should correspond
    /// to the Steam app ID provided by Valve.
    pub fn new(app_id: impl Into<AppId>) -> Self {
        Self(app_id.into())
    }
}

impl Plugin for SteamworksPlugin {
    fn build(&self, app: &mut App) {
        if app.world.contains_resource::<Client<ClientManager>>() {
            bevy_log::warn!("Attempted to add the Steamworks plugin multiple times!");
            return;
        }
        match Client::init_app(self.0) {
            Err(err) => bevy_log::error!("Failed to initialize Steamworks client: {}", err),
            Ok((client, single)) => {
                app.insert_resource(client.clone())
                    .insert_non_send_resource(single)
                    .add_system(run_steam_callbacks.label(SteamworksSystem::RunCallbacks));

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

/// A set of [`SystemLabel`]s for systems used by [`SteamworksPlugin`]
///
/// [`SystemLabel`]: bevy_ecs::system::SystemLabel
#[derive(Debug, Clone, Copy, Eq, Hash, SystemLabel, PartialEq)]
pub enum SteamworksSystem {
    /// A system that runs the Steam SDK callbacks. Anything dependent on
    /// Steam API results should run after this.
    RunCallbacks,
    /// A set of systems for flushing events from the Steam SDK into bevy.
    /// If using [`EventReader`] with any of these events, it should be
    /// scheduled after these systems.
    FlushEvents,
}

fn run_steam_callbacks(client: NonSend<SingleClient>) {
    client.run_callbacks();
}

fn flush_events<T: Send + Sync + 'static>(
    events: ResMut<SteamEvents<T>>,
    mut output: EventWriter<T>,
) {
    let mut pending = events.pending.lock();
    if !pending.is_empty() {
        output.send_batch(pending.drain(0..));
    }
}

fn add_event<T: Callback + Send + Sync + 'static>(app: &mut App, client: &Client<ClientManager>) {
    let pending = Arc::new(Mutex::new(Vec::new()));
    let pending_in = pending.clone();
    app.add_event::<T>()
        .insert_resource(SteamEvents::<T> {
            _callback: client.register_callback::<T, _>(move |evt| {
                pending_in.lock().push(evt);
            }),
            pending,
        })
        .add_system(
            flush_events::<T>
                .label(SteamworksSystem::FlushEvents)
                .after(SteamworksSystem::RunCallbacks),
        );
}

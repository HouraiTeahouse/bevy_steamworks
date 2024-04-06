#![deny(missing_docs)]

//! This crate provides a [Bevy](https://bevyengine.org/) plugin for integrating with
//! the Steamworks SDK.
//!
//! The underlying steamworks crate comes bundled with the redistributable dynamic
//! libraries a compatible version of the SDK. Currently it's v153a.
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
//!       // it is important to add the plugin before `RenderPlugin` that comes with `DefaultPlugins`
//!       .add_plugins(SteamworksPlugin::init_app(480).unwrap())
//!       .add_plugins(DefaultPlugins)
//!       .run()
//! }
//! ```
//!
//! The plugin adds `Client` as a Bevy ECS resource, which can be
//! accessed like any other resource in Bevy. The client implements `Send` and `Sync`
//! and can be used to make requests via the SDK from any of Bevy's threads.
//!
//! The plugin will automatically call `SingleClient::run_callbacks` on the Bevy
//! every tick in the `First` schedule, so there is no need to run it manually.  
//!
//! All callbacks are forwarded as `Events` and can be listened to in the a
//! Bevy idiomatic way:
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
//!       // it is important to add the plugin before `RenderPlugin` that comes with `DefaultPlugins`
//!       .add_plugins(SteamworksPlugin::init_app(480).unwrap())
//!       .add_plugins(DefaultPlugins)
//!       .add_systems(Startup, steam_system)
//!       .run()
//! }
//! ```

use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use bevy_app::{App, First, Plugin};
use bevy_ecs::{
    event::EventWriter,
    prelude::Event,
    schedule::*,
    system::{Res, ResMut, Resource},
};
use bevy_utils::{synccell::SyncCell, syncunsafecell::SyncUnsafeCell};
// Reexport everything from steamworks except for the clients
pub use steamworks::{
    networking_messages, networking_sockets, networking_utils, restart_app_if_necessary, AccountId,
    AppIDs, AppId, Apps, AuthSessionError, AuthSessionTicketResponse, AuthSessionValidateError,
    AuthTicket, Callback, CallbackHandle, ChatMemberStateChange, ComparisonFilter,
    CreateQueryError, DistanceFilter, DownloadItemResult, FileType,
    FloatingGamepadTextInputDismissed, FloatingGamepadTextInputMode, Friend, FriendFlags,
    FriendGame, FriendState, Friends, GameId, GameLobbyJoinRequested, GameOverlayActivated,
    GamepadTextInputDismissed, GamepadTextInputLineMode, GamepadTextInputMode, Input, InstallInfo,
    InvalidErrorCode, ItemState, Leaderboard, LeaderboardDataRequest, LeaderboardDisplayType,
    LeaderboardEntry, LeaderboardScoreUploaded, LeaderboardSortMethod, LobbyChatUpdate,
    LobbyDataUpdate, LobbyId, LobbyKey, LobbyKeyTooLongError, LobbyListFilter, LobbyType, Manager,
    Matchmaking, MicroTxnAuthorizationResponse, NearFilter, NearFilters, Networking,
    NotificationPosition, NumberFilter, NumberFilters, OverlayToStoreFlag, P2PSessionConnectFail,
    P2PSessionRequest, PersonaChange, PersonaStateChange, PublishedFileId, PublishedFileVisibility,
    QueryHandle, QueryResult, QueryResults, RemotePlay, RemotePlayConnected,
    RemotePlayDisconnected, RemotePlaySession, RemotePlaySessionId, RemoteStorage, SIResult,
    SResult, SendType, Server, ServerManager, ServerMode, SteamAPIInitError, SteamDeviceFormFactor,
    SteamError, SteamFile, SteamFileInfo, SteamFileReader, SteamFileWriter, SteamId,
    SteamServerConnectFailure, SteamServersConnected, SteamServersDisconnected, StringFilter,
    StringFilterKind, StringFilters, TicketForWebApiResponse, UGCContentDescriptorID, UGCQueryType,
    UGCStatisticType, UGCType, UpdateHandle, UpdateStatus, UpdateWatchHandle, UploadScoreMethod,
    User, UserAchievementStored, UserList, UserListOrder, UserRestriction, UserStats,
    UserStatsReceived, UserStatsStored, Utils, ValidateAuthTicketResponse, RESULTS_PER_PAGE, UGC,
};

#[derive(Resource)]
struct SteamEvents {
    _callbacks: Vec<CallbackHandle>,
    pending: Arc<SyncUnsafeCell<Vec<SteamworksEvent>>>,
}

/// A Bevy-compatible wrapper around various Steamworks events.
#[derive(Event)]
#[allow(missing_docs)]
pub enum SteamworksEvent {
    AuthSessionTicketResponse(steamworks::AuthSessionTicketResponse),
    DownloadItemResult(steamworks::DownloadItemResult),
    GameLobbyJoinRequested(steamworks::GameLobbyJoinRequested),
    LobbyChatUpdate(steamworks::LobbyChatUpdate),
    P2PSessionConnectFail(steamworks::P2PSessionConnectFail),
    P2PSessionRequest(steamworks::P2PSessionRequest),
    PersonaStateChange(steamworks::PersonaStateChange),
    SteamServerConnectFailure(steamworks::SteamServerConnectFailure),
    SteamServersConnected(steamworks::SteamServersConnected),
    SteamServersDisconnected(steamworks::SteamServersDisconnected),
    TicketForWebApiResponse(steamworks::TicketForWebApiResponse),
    UserAchievementStored(steamworks::UserAchievementStored),
    UserStatsReceived(steamworks::UserStatsReceived),
    UserStatsStored(steamworks::UserStatsStored),
    ValidateAuthTicketResponse(steamworks::ValidateAuthTicketResponse),
}

macro_rules! register_event_callbacks {
    ($client: ident, $($event_name: ident),+) => {
        {
            let pending = Arc::new(SyncUnsafeCell::new(Vec::new()));
            SteamEvents {
                _callbacks: vec![
                    $({
                        let pending_in = pending.clone();
                        $client.register_callback::<steamworks::$event_name, _>(move |evt| {
                            // SAFETY: The callback is only called during `run_steam_callbacks` which cannot run
                            // while any of the flush_events systems are running. This cannot alias.
                            unsafe {
                                (&mut *pending_in.get()).push(SteamworksEvent::$event_name(evt));
                            }
                        })
                    }),+
                ],
                pending,
            }
        }
    };
}

/// A Bevy compatible wrapper around [`steamworks::Client`].
///
/// Automatically dereferences to the client so it can be transparently
/// used.
///
/// For more information on how to use it, see [`steamworks::Client`].
#[derive(Resource, Clone)]
pub struct Client(steamworks::Client);

impl Deref for Client {
    type Target = steamworks::Client;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Resource)]
struct SingleClient(SyncCell<steamworks::SingleClient>);

/// A Bevy [`Plugin`] for adding support for the Steam SDK.
pub struct SteamworksPlugin {
    steam: Mutex<Option<(steamworks::Client, steamworks::SingleClient)>>,
}

impl SteamworksPlugin {
    /// Creates a new `SteamworksPlugin`. The provided `app_id` should correspond
    /// to the Steam app ID provided by Valve.
    pub fn init_app(app_id: impl Into<AppId>) -> Result<Self, SteamAPIInitError> {
        Ok(Self {
            steam: Mutex::new(Some(steamworks::Client::init_app(app_id.into())?)),
        })
    }

    /// Creates a new `SteamworksPlugin` using the automatically determined app ID.
    /// If the game isn't being run through steam this can be provided by placing a steam_appid.txt
    /// with the ID inside in the current working directory.
    /// Alternatively, you can use `SteamworksPlugin::init_app(<app_id>)` to force a specific app ID.
    pub fn init() -> Result<Self, SteamAPIInitError> {
        Ok(Self {
            steam: Mutex::new(Some(steamworks::Client::init()?)),
        })
    }
}

impl Plugin for SteamworksPlugin {
    fn build(&self, app: &mut App) {
        let (client, single) = self
            .steam
            .lock()
            .unwrap()
            .take()
            .expect("The SteamworksPlugin was initialized more than once");

        app.insert_resource(Client(client.clone()))
            .insert_resource(SingleClient(SyncCell::new(single)))
            .insert_resource(register_event_callbacks!(
                client,
                AuthSessionTicketResponse,
                DownloadItemResult,
                GameLobbyJoinRequested,
                LobbyChatUpdate,
                P2PSessionConnectFail,
                P2PSessionRequest,
                PersonaStateChange,
                SteamServerConnectFailure,
                SteamServersConnected,
                SteamServersDisconnected,
                TicketForWebApiResponse,
                UserAchievementStored,
                UserStatsReceived,
                UserStatsStored,
                ValidateAuthTicketResponse
            ))
            .add_event::<SteamworksEvent>()
            .configure_sets(First, SteamworksSystem::RunCallbacks)
            .add_systems(
                First,
                run_steam_callbacks
                    .in_set(SteamworksSystem::RunCallbacks)
                    .before(bevy_ecs::event::EventUpdates),
            );
    }
}

/// A set of [`SystemSet`]s for systems used by [`SteamworksPlugin`]
///
/// [`SystemSet`]: bevy_ecs::schedule::SystemSet
#[derive(Debug, Clone, Copy, Eq, Hash, SystemSet, PartialEq)]
pub enum SteamworksSystem {
    /// A system set that runs the Steam SDK callbacks. Anything dependent on
    /// Steam API results should scheduled after this. This runs in
    /// [`First`].
    RunCallbacks,
}

fn run_steam_callbacks(
    mut client: ResMut<SingleClient>,
    events: Res<SteamEvents>,
    mut output: EventWriter<SteamworksEvent>,
) {
    client.0.get().run_callbacks();
    // SAFETY: The callback is only called during `run_steam_callbacks` which cannot run
    // while any of the flush_events systems are running. The system is registered only once for
    // the client. This cannot alias.
    let pending = unsafe { &mut *events.pending.get() };
    if !pending.is_empty() {
        output.send_batch(pending.drain(0..));
    }
}

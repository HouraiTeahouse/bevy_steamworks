use bevy::prelude::*;
use bevy_steamworks::*;

fn steam_system(steam_client: Res<Client>) {
    for friend in steam_client.friends().get_friends(FriendFlags::IMMEDIATE) {
        println!(
            "Friend: {:?} - {}({:?})",
            friend.id(),
            friend.name(),
            friend.state()
        );
    }
}

fn main() {
    // Use the demo Steam AppId for SpaceWar
    App::new()
        // it is important to add the plugin before `RenderPlugin` that comes with `DefaultPlugins`
        .add_plugins(SteamworksPlugin::init_app(480).unwrap())
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, steam_system)
        .run()
}

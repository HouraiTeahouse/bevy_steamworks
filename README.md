# bevy-steamworks

[![crates.io](https://img.shields.io/crates/v/bevy-steamworks.svg)](https://crates.io/crates/bevy-steamworks)
[![Documentation](https://docs.rs/bevy-steamworks/badge.svg)](https://docs.rs/bevy-steamworks)
![License](https://img.shields.io/crates/l/bevy-steamworks.svg)

This crate provides a [Bevy](https://bevyengine.org/) plugin for integrating with
the Steamworks SDK.

## Installation
Add the following to your `Cargo.toml`:

```toml
[dependencies]
bevy-steamworks = "0.7"
```

The steamworks crate comes bundled with the redistributable dynamic libraries
of a compatible version of the SDK. Currently it's v153a.

## Usage

To add the plugin to your app, simply add the `SteamworksPlugin` to your
`App`. This will require the `AppId` provided to you by Valve for initialization.

```rust no_run
use bevy::prelude::*;
use bevy_steamworks::*;

fn main() {
  // Use the demo Steam AppId for SpaceWar
  App::new()
      .add_plugins(DefaultPlugins)
      .add_plugin(SteamworksPlugin::new(AppId(480)))
      .run()
}
```

The plugin adds `steamworks::Client` as a Bevy ECS resource, which can be
accessed like any other resource in Bevy. The client implements `Send` and `Sync`
and can be used to make requests via the SDK from any of Bevy's threads. However,
any asynchronous callbacks from Steam will only run on the main thread.

The plugin will automatically call `SingleClient::run_callbacks` on the Bevy
main thread every frame in `CoreStage::First`, so there is no need to run it
manually.

**NOTE**: If the plugin fails to initialize (i.e. `Client::init()` fails and
returns an error, an error wil lbe logged (via `bevy_log`), but it will not
panic. In this case, it may be necessary to use `Option<Res<Client>>` instead.

All callbacks are forwarded as `Events` and can be listened to in the a
Bevy idiomatic way:

```rust no_run
use bevy::prelude::*;
use bevy_steamworks::*;

fn steam_system(steam_client: Res<Client>) {
  for friend in steam_client.friends().get_friends(FriendFlags::IMMEDIATE) {
    println!("Friend: {:?} - {}({:?})", friend.id(), friend.name(), friend.state());
  }
}

fn main() {
  // Use the demo Steam AppId for SpaceWar
  App::new()
      .add_plugins(DefaultPlugins)
      .add_plugin(SteamworksPlugin::new(AppId(480)))
      .add_startup_system(steam_system)
      .run()
}
```

## Bevy Version Supported
 
|Bevy Version |bevy\_steamworks|
|:------------|:---------------|
|0.10         |0.7             |
|0.9          |0.6             |
|0.8          |0.5             |
|0.7          |0.4             |
|0.6          |0.2, 0.3        |
|0.5          |0.1             |

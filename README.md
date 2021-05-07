# bevy-steamworks

[![crates.io](https://img.shields.io/crates/v/bevy-steamworks.svg)](https://crates.io/crates/bevy-steamworks)
[![Documentation](https://docs.rs/bevy-steamworks/badge.svg)](https://docs.rs/bevy-steamworks)
[![License](https://img.shields.io/crates/l/bevy-steamworks.svg)

This crate provides a [Bevy](https://bevyengine.org/) plugin for integrating with
the Steamworks SDK via the steamworks crate.

## Installation
Add the following to your `Cargo.toml`:

```toml
[dependencies]
bevy-steamworks = "0.1.0"
```

Ensure that your computer has all the needed [requirements](https://rust-lang.github.io/rust-bindgen/requirements.html) to use [bindgen](https://github.com/rust-lang/rust-bindgen).

Download and install the [steamworks sdk](https://partner.steamgames.com/doc/sdk)
and set the environment variable `STEAM_SDK_LOCATION` to point to it.

At runtime, a "steam_appid.txt" file with the registered Steam App ID of the game
is required in the same directory as the executable.

## Usage

To add the plugin to your game, simply add the `SteamworksPlugin` to your
`AppBuilder`.

```rust
use bevy::prelude::*;
use bevy_steamworks::SteamworksPlugin;

fn main() {
  App::build()
      .add_plugins(DefaultPlugins)
      .add_plugin(SteamworksPlugin)
      .run()
}
```

The plugin adds `steamworks::Client` as a Bevy ECS resource, which can be
accessed like any other resource in Bevy. The client implements `Send` and `Sync`
and can be used to make requests via the SDK from any of Bevy's threads. However,
any asynchronous callbacks from Steam will only run on the main thread.

The plugin will automatically call `SingleClient::run_callbacks` on the Bevy
main thread every frame, so there is no need to run it manually.

**NOTE**: If the plugin fails to initialize (i.e. `Client::init()` fails and
returns an error, an error wil lbe logged (via `bevy_log`), but it will not
panic. In this case, it may be necessary to use `Option<Res<Client>>` instead.

```rust
use bevy_steamworks::{Client, FriendFlags};

fn steam_system(steam_client: Res<Client>) {
  for friend in client.friends().get_friends(FriendFlags::IMMEDIATE) {
    println!("Friend: {:?} - {}({:?})", friend.id(), friend.name(), friend.state());
  }
}

fn main() {
  App::build()
      .add_plugins(DefaultPlugins)
      .add_plugin(SteamworksPlugin)
      .add_startup_system(steam_system.system())
      .run()
}
```

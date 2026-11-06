<!--
type: tutorial
description: The smallest Enshwick mod in each form, a data pack, a Luau script and a Rust mod, each a few lines that show a result in the log and in the game.
-->

# Hello, world

You make the smallest mod in each of its three forms: a data pack, a Luau
script and a Rust mod. Each takes a few lines and shows its result in the
log and in the game.

## What you need

Every form needs the Enshwick loader in the game's folder. Take
`enshwick.zip` of the newest alpha on the
[releases page](https://github.com/enshwick/enshwick/releases) and extract
it next to the game's executable.

Starting a mod and building one in Rust also need Rust 1.99 or newer, from
[rustup](https://rustup.rs). Install the Windows target and the
[`cargo enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/cargo-enshwick)
tool:

```bash
rustup target add x86_64-pc-windows-msvc
cargo install --git https://github.com/enshwick/enshwick cargo-enshwick
```

A Rust mod is a Windows DLL. On Windows it builds with the MSVC build
tools. On Linux or macOS it builds with
[cargo-xwin](https://github.com/rust-cross/cargo-xwin), which fetches the
Windows SDK: `cargo install --locked cargo-xwin`.

## A data pack

A data pack is a folder of TOML files with no code. This one gives each
player a torch the first time they join a world.

1. Make a folder `you.hello-pack` with two files. `pack.toml` names the
   pack:

   ```toml
   id = "you.hello-pack"
   version = "0.1.0"
   ```

   `rewards.toml` holds the gift:

   ```toml
   [[reward]]
   id = "hello"
   on = { join = true }
   give = [{ item = "Tool_Torch01", count = 1 }]
   limit = "once"
   ```

2. Copy the folder into the game's `mods/` folder, on the server and in
   every player's game.
3. Start the game and load a world. The pack's log,
   `logs/you.hello-pack_<date>_<time>.log` in the game's folder, says what
   the loader read:

   ```text
   info  [you.hello-pack] data pack: 0 ids, 0 loot drops, 0 changes, 0 selections
   ```

4. Join the world: a torch is in your inventory.

`cargo enshwick new --data you.hello-pack` makes the same folder with one
commented file per kind of data. [A mod without Rust](content/a-mod-without-rust.md)
goes on from here.

## A Luau script

A Luau mod is a data pack with scripts. This one writes a line to its log
and greets each player who joins.

1. Run `cargo enshwick new --luau you.hello-luau`. It makes the folder
   `you.hello-luau/` with `main.luau`, a manifest and the API's types for
   your editor.
2. Replace `main.luau` with:

   ```luau
   --!strict
   enshwick.log.info("Hello, world")

   enshwick.on("PlayerJoined", function(joined)
       local player = enshwick.world.player(joined.entity)
       if player then
           enshwick.world.notify(player, "Hello, world")
       end
   end)
   ```

3. Copy the folder into the game's `mods/` folder on the server.
4. Start the server. Its log, `logs/you.hello-luau_<date>_<time>.log`, has
   the line:

   ```text
   info  [you.hello-luau] Hello, world
   ```

5. Join the world: "Hello, world" shows on your screen.

[Luau mods](luau.md) tells what a script can reach and how it runs.

## A Rust mod

A Rust mod is a DLL built against the
[`enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick)
crate. This one writes the same line and greets each player who joins.

1. Run `cargo enshwick new you.hello-rust`. It makes the crate
   `hello-rust/`.
2. The greeting uses [unstable](stability.md) calls. In `Cargo.toml`, ask
   for them:

   ```toml
   [dependencies]
   enshwick = { workspace = true, features = ["unstable"] }
   ```

3. In `src/lib.rs`, keep the first line, the comment that `new` wrote.
   Replace the rest with the code that follows this list.
4. Build the mod and install it into the game's folder:

   ```bash
   cd hello-rust
   cargo enshwick install --game "C:/Program Files (x86)/Steam/steamapps/common/Enshrouded"
   ```

5. Start the server. Its log, `logs/you.hello-rust_<date>_<time>.log`, has
   the line:

   ```text
   info  [you.hello-rust] Hello, world
   ```

6. Join the world: "Hello, world" shows on your screen.

The code of `src/lib.rs`:

<!-- smallest-mod -->
```rust,standalone_crate
use enshwick::events::PlayerJoined;
use enshwick::{Load, Mod, Off};

/// The mod.
#[derive(Debug)]
pub struct Hello;

impl Mod for Hello {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        enshwick::info!("Hello, world");
        let world = load.world()?;
        load.on::<PlayerJoined>(move |joined| {
            if let Some(player) = world.player(joined.entity) {
                world.notify(&player, "Hello, world");
            }
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Hello);
```
<!-- /smallest-mod -->

What each piece does:

- `init` runs once, as the loader starts the mod. It writes the line, then
  subscribes to [`PlayerJoined`](events.md).
- `load.world()` is the handle to the server's [world](world.md). The
  handler moves it in and shows the joining player a notice.
- Any `?` that fails turns the mod off, with the reason in the log, and the
  game runs on without it ([The life of a mod](concepts/lifecycle.md)).
- `declare_mod!` exports the mod to the loader.

## What `cargo enshwick new` makes

The id is `<author>.<name>`: your name or handle, a dot, and the mod's
name. Each part is of `[a-z0-9-]`, at most 64 characters in all, and the
name starts with a letter, since it names the crate too. The author part
keeps two mods of one name apart.

`--side` says where the mod runs: `server`, the default, or `both` when
players' games need it too ([Sides and hosting](concepts/sides-and-hosting.md)).
`--dir` sets the folder the crate goes into, the current one by default.

The crate holds:

- `Cargo.toml`, with the mod's id, side and name in
  `[package.metadata.enshwick]`, its
  [manifest](concepts/loader-and-abi.md#the-manifest);
- `src/lib.rs`, a mod with one setting, `enabled`, and two tests that run
  on the [fake loader](testing.md);
- `src/texts.toml`, the mod's name, description and setting labels in
  English and Ukrainian ([Texts](content/texts.md));
- `rust-toolchain.toml` and `.cargo/config.toml`, which set the Windows
  target and link the C runtime statically, so players need no
  redistributable;
- `README.md` and `.gitignore`.

The release profile keeps `panic = "unwind"`. The loader needs it to catch
a panic in a mod and turn only that mod off.

Inside a Cargo workspace, `new` adds the crate to the workspace's
`members`. Cargo reads the release profile, the toolchain file and
`.cargo/config.toml` only at the workspace root, so put `panic = "unwind"`
and the static C runtime there.

`cargo enshwick install` builds the mod in release for the Windows target,
through `cargo xwin` off Windows. It copies the DLL to
`<game>/mods/<id>/`. The game folder is `--game` or the `ENSHROUDED_DIR`
environment variable. In a workspace with several mods, `-p` names one.

`cargo enshwick manifest <dll>` prints what a built DLL says about itself:
id, version, side, join check and dependencies. It loads the DLL, so it
runs on Windows only.

## Where the logs are

The loader writes to `logs/` in the game's folder, one file per source and
run. `enshwick_<date>_<time>.log` says which mods loaded, were skipped or
turned off, and why. `<mod id>_<date>_<time>.log` holds the mod's own
lines. [Log files](logging.md#log-files) has the rest.

## What to read next

[Your first real mod](first-mod.md) adds an item, its recipe, its texts and
a setting, step by step.

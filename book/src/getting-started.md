# Getting started

You set up the tools, create a mod, put it into the game and find its log.
The first mod uses the API's high-level calls. Raw hooks come last, for what
the API has no call for. For a mod without Rust, start at
[Luau mods](luau.md) or [A mod without Rust](content/a-mod-without-rust.md).

## Performance

Mods run inside the game's frame, so their handlers cost the game time. A
tick handler runs every 100 ms by default, since a call costs about 1 µs and
the number of server steps a second differs between servers, clients and
load. Ask for every step only for state that changes each step, input or
the camera. [Ticks](events.md#ticks) has the numbers, the every-step and
runtime period options, and an adaptive tick.

## The toolchain

A mod is a Windows DLL. You need:

- Rust 1.99 or newer through [rustup](https://rustup.rs), with the target
  `x86_64-pc-windows-msvc`;
- on Windows, the MSVC build tools; on Linux or macOS,
  [cargo-xwin](https://github.com/rust-cross/cargo-xwin)
  (`cargo install --locked cargo-xwin`), which fetches the Windows SDK;
- the Enshwick loader in the game's folder: extract
  [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip),
  the development build until the first release, 1.0.0, next to
  `enshrouded.exe` or `enshrouded_server.exe`.

```bash
rustup target add x86_64-pc-windows-msvc
cargo install --git https://github.com/enshwick/enshwick cargo-enshwick
```

The second line installs
[`cargo enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/cargo-enshwick),
a Cargo subcommand that starts a mod, installs it and reads a built one.

## A new mod

```bash
cargo enshwick new you.my-mod --side server
```

`new <id>` creates the crate in `<dir>/<name>`; `--dir` sets the folder (the
current one by default) and `--side` is `server` (the default), `client` or
`both`. The id is `<author>.<name>`: your name or handle, a dot, and the
mod's name, each of `[a-z0-9-]`, at most 64 characters in all. The author part
keeps two mods of one name apart. The name starts with a letter, since it names
the crate too. The crate is the one the
[mod template](https://github.com/enshwick/template-mod) makes, without the
template's repository tools. It holds:

- `Cargo.toml`: a `cdylib` named after the id's name, `enshwick` pinned with
  `=` to the release the tool was built with, `serde`, the lints, and a
  release profile with `panic = "unwind"`, which every hook needs to catch
  panics;
- `src/lib.rs`: a settings type with one field, `enabled`, a mod type named
  after the id's name (`my-mod` becomes `MyMod`), its manifest,
  `declare_mod!` and two tests: every text exists in every language, and the
  mod starts on the [fake loader](testing.md);
- `src/texts.toml`: the mod's name, description and setting labels;
- `rust-toolchain.toml`, with the Windows target, and `.cargo/config.toml`,
  which links the C runtime statically, so players need no redistributable;
- `README.md` and `.gitignore`.

`--enshwick-rev <commit>` builds the mod on a commit of this repository's
`main` branch from git instead of the release, as the rolling `dev` loader is
built.

Inside a Cargo workspace the crate joins it: `new` adds its folder to the
workspace's `members`, unless a pattern there already covers it. The release
profile, the toolchain file and `.cargo/config.toml` stay out, since Cargo
reads them only at the workspace root; put `panic = "unwind"` and the static C
runtime there. Lints and dependencies come from the workspace where it
declares them. A folder the workspace excludes gets a workspace of its own.

It builds and loads as it is: on first start it writes
`mod-configs/my-mod.toml` and logs one line.

## Install it into the game

```bash
cd my-mod
cargo enshwick install --game "C:/Program Files (x86)/Steam/steamapps/common/Enshrouded"
```

`install` builds the mod in release for `x86_64-pc-windows-msvc`, through
`cargo xwin` off Windows, and copies the DLL to `<game>/mods/<package>.dll`.
When the target or cargo-xwin is missing, it says so with the command that
adds it. The game folder is `--game`, or the
`ENSHROUDED_DIR` environment variable. It must hold `enshrouded.exe` or
`enshrouded_server.exe`. In a workspace with several mods, `--package` (`-p`)
names one; without it, `install` takes the package in the current folder, or
the workspace's only mod. It also says when the loader is not installed in
that folder yet.

To see what a built DLL says about itself (id, version, side, join check and
dependencies):

```bash
cargo enshwick manifest target/x86_64-pc-windows-msvc/release/my_mod.dll
```

`--expect-version <version>` makes it fail when the DLL reports another
version, for release pipelines. It loads the DLL, so it runs on Windows only.

## A first mod

This mod sets how many Flame Altars every Flame level allows. The API has a
call for exactly that, so the mod only answers with a number from its
settings.

```rust,standalone_crate
use std::sync::atomic::{AtomicU8, Ordering};

use enshwick::{Load, Mod, Off};

/// Flame Altar settings.
#[derive(serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Altars allowed at every Flame level, 1 to 16.
    altars: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { altars: 16 }
    }
}

/// What the limit answers; replaced whole when the settings change.
static ALTARS: AtomicU8 = AtomicU8::new(16);

fn apply(settings: &Settings) {
    ALTARS.store(settings.altars.clamp(1, 16), Ordering::Relaxed);
}

struct AltarCount;

impl Mod for AltarCount {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.my-altar-count", side: BOTH, join: true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        apply(&settings);
        load.on_altar_limit(|limit| limit.allowed = ALTARS.load(Ordering::Relaxed))?;
        load.on_config_change(|settings: Settings| apply(&settings))?;
        enshwick::info!("{} altars per Flame level", settings.altars);
        Ok(Self)
    }
}

enshwick::declare_mod!(AltarCount);
```

What each piece does:

- `Settings` derives [`Config`](settings.md): the file
  `mod-configs/you.my-altar-count.toml` is written from its doc comments and
  `Default` on first start, and `load.config()` reads it.
- `manifest!` names the mod and where it runs. `side: BOTH` and
  `join: true` because the client's interface asks the same limit, so
  players and their server must agree. See
  [The loader and the ABI](concepts/loader-and-abi.md).
- `init` reads the settings and adds its answer to the game's limit with
  `on_altar_limit`, which the loader asks with the Flame level, the game's own
  count and the count so far each time the game needs the number. Any `?` that fails turns the mod off with the reason in
  the log; the game then runs vanilla. See
  [The life of a mod](concepts/lifecycle.md).
- `on_config_change` applies an edited file while the game runs. The limit
  reads an atomic, so the handler can change it under a running game.
- `declare_mod!` exports the mod and adds `AltarCount::instance()`.

[`enshwick-better-altars`](https://github.com/enshwick/mod-better-altars)
started as this mod. It adds a table per Flame level swapped with
`arc-swap`, the table as a join value, and more than 16 altars.

Most of what mods do has such a call: [events](events.md), the
[world](world.md), [content](content.md), [settings](settings.md) and the
other chapters of Writing a mod. Look there first.

## Reaching lower

When the API has no call for what a mod needs, the mod reaches into the game
itself: it finds a function by its symbolic id and links a hook into it.
This is the low-level way. It is `unsafe`: the mod states the function's
type, and the compiler cannot check it. The same limit as a hook on the game's own lookup:

```rust,standalone_crate
use std::sync::atomic::{AtomicU8, Ordering};

use enshwick::{Load, Mod, Next, Off};

static ALTARS: AtomicU8 = AtomicU8::new(16);

struct AltarCount;

enshwick::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, level: u32) -> u8 = max_altars;
}

fn max_altars(next: Next<'_, unsafe extern "system" fn(u64, u32) -> u8>, balancing: u64, level: u32) -> u8 {
    match AltarCount::instance() {
        Some(_) => ALTARS.load(Ordering::Relaxed),
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.call(balancing, level) },
    }
}

impl Mod for AltarCount {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.my-altar-count", side: BOTH, join: true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let max_altars = load.symbol("building.max_altars")?;
        // SAFETY: the game's altar limit has the type `MAX_ALTARS` declares.
        unsafe { load.hook(&MAX_ALTARS, max_altars) }?;
        Ok(Self)
    }
}

enshwick::declare_mod!(AltarCount);
```

- `detour!` declares a hook: a static `MAX_ALTARS` and an entry point that
  catches panics. The body gets `next`, the rest of the chain, and the
  function's arguments. See [Hooks](hooks.md).
- `load.symbol` finds the function by its symbolic id on the running build.
  [Finding the game](finding-the-game.md) shows how ids are found and pinned.
- A hook chains with other mods' hooks on the same function, as
  `on_altar_limit` filters chain. The mod must also keep its hook working
  when the game changes. Prefer the API's call where there is
  one, and ask for one when other mods would need the same hook.

## Where the logs are

The loader writes to `logs/enshwick/` in the game's folder, starting each
file afresh at every start:

- `all.log`: every line of the loader and every mod;
- `loader.log`: the loader's lines, such as which mods loaded, were skipped or
  turned off, and why;
- `mods/<id>.log`: one mod's lines;
- `status.log` and `crash.log`: see
  [Logging and diagnostics](logging.md).

A mod writes there with `enshwick::info!` and the other log macros.
`mod-configs/enshwick.toml` sets how much is kept.

## What to read next

Read [Concepts](concepts.md) once: how the loader finds and starts mods, the
two sides of the game, the join check and threads. Then take the chapter for
what your mod does: [Settings](settings.md), [Events](events.md),
[Hooks](hooks.md), [Content](content.md). [Testing](testing.md) shows how to
test a mod without the game.

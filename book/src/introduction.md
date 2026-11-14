<!--
type: explanation
description: Enshwick is the modding framework and API for Enshrouded. Its mod loader runs every mod; this book is for the people who write mods with it.
-->

# Introduction

Enshwick is the modding framework and API for
[Enshrouded](https://enshrouded.com). This book is for people who write mods
with it. The Enshwick mod loader comes with the framework and runs every mod,
so players install one thing.

## What a mod is

A mod takes one of four forms:

- a Rust mod: a `cdylib` built against one crate,
  [`enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick),
  with a typed API;
- a [Luau mod](luau.md): a folder of scripts, with no compiler;
- a [data pack](content/packs.md): a folder of TOML files, with no code;
- a [C or C++ mod](mods-in-c.md): a DLL built against the C header of the
  same ABI.

One tool,
[`cargo enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/cargo-enshwick),
starts each of the first three. The crates under
[`crates/internal`](https://github.com/enshwick/enshwick/tree/main/crates/internal)
are parts of the loader, and a mod never depends on them.

A mod names itself in a manifest. It reads its settings, reacts to events, adds
items and recipes, and offers players actions through its controls. It can
carry its texts in every language the game offers ([Texts](content/texts.md)).
Its unit tests run without the game against a fake host ([Testing](testing.md)).
Where the API has no call, a Rust mod links hooks into game functions. When a
mod fails, the loader turns that mod off, and the game runs vanilla where the
mod acted.

## Supported languages for mods

Rust and Luau reach the same API by the same names. A data pack needs no code.
C and C++ mods include the C header `enshwick.h`. In CI, examples in both languages
start on the fake host whenever the header changes ([Mods in C and C++](mods-in-c.md)).
Any other language that builds a Windows DLL and can call a C ABI works the
same way, but no other language is tested.

## What a mod gets from Enshwick

- **Native speed for native mods.** Rust, C and C++ mods run as machine code
  inside the game, with no garbage collector. Luau scripts run in an
  interpreter and cost more per call ([Performance](concepts/performance.md)).
- **No update for a game update.** The loader reads the game's own description
  of its data at every start. It turns off only what it cannot find
  ([Why a game update needs no Enshwick update](concepts/game-updates.md)).
- **An API across the game.** Items, recipes, loot, the world, players and the
  interface each have calls; [API coverage](https://enshwick.com/dev/api-coverage/)
  shows each game area. Where the API has no call, a mod reaches the game
  directly ([Before you reach into the game](reaching-into-the-game.md)).
- **A failure stays with its mod.** A panic turns off the mod and leaves the
  game running. When a hardware fault ends the game, the crash file names the
  mod ([The life of a mod](concepts/lifecycle.md)).
- **Mods that work together.** A mod offers other mods typed services, events
  and extension points ([Services between mods](services.md)).

## How to read this book

The book leads from the simplest mod to the hardest parts. Start with
[Hello, world](getting-started.md) and [Your first real mod](first-mod.md).
Everyday tasks and Game systems each cover one subject; read the one you
need. [Reaching into the game](reaching-into-the-game.md) is for what the API
has no call for, and [Concepts](concepts.md) explains how the loader runs
mods. The API's rustdoc is the reference for every type and function
(`cargo doc -p enshwick --features test --open` builds it). This book says how
they fit together. Its links to the API point at the source the rustdoc is built
from.

The examples in
[`examples/`](https://github.com/enshwick/enshwick/tree/main/examples) and our
mods, one folder each in the [mods repository](https://github.com/enshwick/mods),
are full working mods.

## Building the book

The book is an [mdBook](https://rust-lang.github.io/mdBook/). From the
repository root:

```bash
cargo install --locked mdbook
mdbook serve book
```

Its Rust examples compile as doctests of `enshwick`, so
`cargo test --doc --workspace` checks them. `rumdl check .` and `typos` check
the text.

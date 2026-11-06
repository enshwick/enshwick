<!--
type: explanation
description: When a mod reaches past the API into the game itself: the ways to do it, from reflected data to code patches, and what each one costs the mod to keep working.
-->

# Before you reach into the game

The API has a call for most of what mods do. When it has none, a Rust mod
reaches into the game itself. Each way to do that costs more to keep
working than the one before it.

## Why the API comes first

An API call keeps working when the game updates, since the loader finds the
game's code for it on every build. It also composes: when several mods
answer the same call, the loader runs them in load order. A mod that reaches
into the game must fit each new build itself. The loader still chains every
mod's hooks on one function, but each link must call on for the others to
run ([Hooks](hooks.md#chains-and-priority)).

The same altar limit, first as the API's call:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_altar_limit(|limit| limit.allowed = 16)
}
```

Then as a hook on the game's own lookup:

```rust,standalone_crate
use enshwick::{Load, Mod, Next, Off};

struct AltarCount;

enshwick::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, level: u32) -> u8 = max_altars;
}

fn max_altars(next: Next<'_, unsafe extern "system" fn(u64, u32) -> u8>, balancing: u64, level: u32) -> u8 {
    match AltarCount::instance() {
        Some(_) => 16,
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.call(balancing, level) },
    }
}

impl Mod for AltarCount {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let max_altars = load.symbol("building.max_altars")?;
        // SAFETY: the game's altar limit has the type `MAX_ALTARS` declares.
        unsafe { load.hook(&MAX_ALTARS, max_altars) }?;
        Ok(Self)
    }
}

enshwick::declare_mod!(AltarCount);
```

The hook states the function's type in `unsafe` code, and the compiler
cannot check it. When a game update changes that function, the mod must
change too. Where a call exists, use it. Where other mods would need the
same hook, ask for a call.

## The ways in, from safest to hardest

| Way | What the mod does | When the game updates |
| --- | --- | --- |
| [The game's own types](game-types.md) | reads and writes components and events by their reflected names | the loader reads the new build's reflection |
| [Game resources as values](resource-values.md) | reads a resource of the game's archive by its type name and serves a changed copy | the loader reads the new build's reflection |
| [Hooks](hooks.md) | puts a function in front of a game function | a system found by name, or a symbol the loader pins, keeps working; a mod's own pinned symbol needs a new row |
| [Mid-function hooks](mid-function-hooks.md) | runs code at one instruction inside a function | a pinned symbol needs a new row, and a byte pattern finds the place while those bytes stay |
| [Code patches](code-patches.md) | rewrites bytes of the game's code | as for a mid-function hook |

[Finding the game](finding-the-game.md) shows how a mod asks the loader for
a function, a field or a value by name, so it never holds an address.
[Code in the game's frame](game-frame.md), [Pacing and threads](pacing.md)
and [ECS inside systems](ecs.md) cover what code inside the game's frame
may do.

A Luau mod has no hooks or code patches: a script runs in a sandbox without
pointers ([Where Luau differs from Rust](luau.md#where-luau-differs-from-rust)).
It reads and writes the game's types and resources as values, through
`enshwick.typed` and `enshwick.resources`.

## What happens when it goes wrong

A lookup that finds nothing returns `Off`: the mod stays off, and the game
runs vanilla where it would have acted. A panic in a hook turns only that
mod off. A hook with the wrong type, or a patch that writes the wrong
bytes, can crash the game, and the crash file names the mod
([The life of a mod](concepts/lifecycle.md)).

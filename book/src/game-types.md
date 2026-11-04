# The game's own types

Every component, event and resource the game reflects has a typed Rust item in
`enshwick::typed`, generated from the game's own reflection data. A mod reads
and changes them as Rust values, by the game's field names. For any other
struct, such as one a [hooked system](ecs.md) hands you, `cargo enshwick types`
writes the game's types into your own crate.

## How do I turn on the typed API?

The items come with the `typed` feature, which builds on `unstable` (see
[Stability](stability.md)):

```toml
[dependencies]
enshwick = { version = "1", features = ["typed"] }
```

The feature adds a few thousand items, so it adds some seconds to a build.

## How are the items named?

A module holds each of the game's namespaces, without the engine's root
namespace: the game's `keen::ecs::Health` is `enshwick::typed::ecs::Health`,
and `keen::LootSettingsResource` is `enshwick::typed::LootSettingsResource`.
A field `deadEntityId` is `dead_entity_id`, and an enum constant `MagicalFog`
is the variant `MagicalFog`. Every enum has a variant `Other` for a constant
the item does not name, such as one a newer build adds. The API reference lists
every item with its fields, size and role.

## Read and write a component

Resolve the component type once at `init`. The resolve checks the type's shape
on the running build, and it fails only for that type. Read an entity's
component on the game thread; `write` writes it back, marked changed the
game's way:

```rust
use enshwick::events::PlayerJoined;
use enshwick::typed::{ComponentType, ecs::Health};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let health = ComponentType::<Health>::resolve(load)?;
    let world = load.world()?;
    load.on::<PlayerJoined>(move |joined| match health.read(world, joined.entity) {
        Ok(Some(value)) => enshwick::info!("health storage {:?}", value.data_storage),
        Ok(None) => {}
        Err(why) => enshwick::warn!("health does not read: {why}"),
    })
}
```

## Handle a game event

An `EventType` calls your handler with each event of its type, read as its
item:

```rust
use enshwick::typed::{EventType, ecs::EntityDiedEvent};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    EventType::<EntityDiedEvent>::resolve(load)?.on(load, |died| {
        enshwick::info!("entity {} died", died.dead_entity_id.id);
    })
}
```

## Change a resource

A `ResourceType` gives the key of a resource by its GUID, reads the game's own
value, and serves a changed one. Several mods that serve one key change it in
load order, each on the one before's result. The bytes of what the item does
not hold stay as the game has them:

```rust
use enshwick::content::Guid;
use enshwick::typed::{LootSettingsResource, ResourceType};

// The GUID of the loot settings to change.
const SETTINGS: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000002");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let loot = ResourceType::<LootSettingsResource>::resolve(load)?;
    let key = loot.key(SETTINGS);
    loot.serve(load, key, key, |settings| {
        settings.modifier.level_offset_range_max += 1.0;
    })
}
```

## What happens when a game update changes a type?

A type the new build changed turns off alone, with a warning that names the
build; the rest of the mod runs. A rebuilt mod gets the new build's items.
[Typed types across game builds](concepts/typed-stability.md) explains each
case.

## Can a Luau mod use them?

A Luau mod reads and writes the same types by their game names through
[game resources as values](resource-values.md); the Luau names of the typed
items are not part of the Luau API yet.

## Write the game's types into your crate

### The game's names

The names are the game's:
`keen::ecs::Health` becomes `game::keen::ecs::Health`, a field `dataStorage`
becomes `data_storage`, and an enum constant `MagicalFog` becomes `MAGICAL_FOG`.

### Write the types

Name the types you need, or a namespace ending in `::`. The types they use come
with them:

```sh
cargo enshwick types --from "$ENSHROUDED_DIR" keen::ecs::Health \
  --rust src/game.rs --luau game.d.luau
```

`--from` takes the game folder or one of its executables. Without names, every
runtime type comes out, over six thousand of them; the editor's `keen::ds::`
copies come only when named. Add `pub mod game;` to `src/lib.rs`.

Luau has no namespaces, and luau-lsp makes every exported type global, so a
game type whose name the [API's definitions](luau.md) already take keeps its
namespace: the game's `keen::ecs::HitEvent` is `Ecs_HitEvent` beside the API's
`HitEvent`. A [Luau mod](luau.md#start-a-luau-mod) loads both files at once.

### Read a struct from game memory

Every struct is `repr(C)` with each field at the offset the game reflects, and
reads through [`enshwick::raw`](finding-the-game.md). A `bool` is its byte and
an enum is a newtype over its integer, so any bytes the game holds read as a
value. Check at `init` that the running build still has the size the module
expects:

```rust
# mod game {
#     pub mod keen {
#         pub mod ecs {
#             /// `keen::ecs::Health`: 40 bytes, aligned to 4.
#             #[repr(C, align(4))]
#             #[derive(Clone, Copy, Debug)]
#             pub struct Health {
#                 _pad_0: [u8; 8],
#                 /// `dataStorage`, at 0x8.
#                 pub data_storage: [i32; 8],
#             }
#             // SAFETY: integers only, so all bytes zero are a valid value.
#             unsafe impl enshwick::raw::Zeroable for Health {}
#             // SAFETY: integers only, so any bytes are a valid value.
#             unsafe impl enshwick::raw::AnyBitPattern for Health {}
#         }
#     }
# }
use game::keen::ecs::Health;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    if load.type_size("keen::ecs::Health")? as usize != size_of::<Health>() {
        return Err(enshwick::Off::new("keen::ecs::Health changed size"));
    }
    Ok(())
}

/// Reads the health component at `component`.
///
/// # Safety
///
/// - `component` points at a live `keen::ecs::Health` no thread writes now.
unsafe fn health(component: *const u8) -> Health {
    // SAFETY: the caller guarantees a whole `Health` at `component`.
    unsafe { enshwick::raw::read_at(component, 0) }
}
```

A field Rust cannot place where the game has it, such as an eight-byte integer
at an offset of four, comes out as bytes; one the game does not describe stays
in padding, and the struct's documentation names it.

### Use them from Luau

`game.d.luau` declares each struct as an exported record type, each enum as a
union of its constants' names, and keeps the game's field names. Point luau-lsp
at it beside `enshwick.d.luau`. It holds types only, and no Luau call returns
game structs.

### Write them again after an update

Offsets and sizes change between game builds, and the file names the build it
came from. Run the command again on the new build and rebuild the mod. Until
then, the size check at `init` turns the mod off on a build where the type grew
or shrank; for a field that matters, compare `load.field_offset(type, field)`
with its offset too.

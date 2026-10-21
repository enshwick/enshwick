# Content

A mod can add game content: items, recipes, crafting stations, texts, and
any other game resource. This part of the book covers the high-level calls
first and the served resources under them last:

- [Items](content/items.md): a new item cloned from a vanilla one, with
  its own name, icon, tooltip lines and meter;
- [Recipes](content/recipes.md): single recipes and derived copies of
  vanilla ones;
- [Workshops](content/workshops.md): a crafting station with a workshop of
  its own;
- [Loot containers](content/loot.md): reading the corpses and loot bags
  enemies leave;
- [Random loot](content/random-loot.md): mod items in random loot, each
  with an exact chance;
- [Served resources](content/resources.md): any resource, new or changed,
  built from bytes;
- [Texts](content/texts.md): names and descriptions per language.

[Inventories](inventories.md), which read and change what players and chests
hold, have a chapter of their own.

## Server and client alike

The game reads its resources on both sides: the server decides what a recipe
costs and what an item does, the client shows the item and its crafting menu.
A mod that adds content runs on both, and server and players must run the
same version of it:

```rust
# struct MyMod;
# impl enshwick_loader_api::Mod for MyMod {
fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
    enshwick_loader_api::manifest!(id: "my-mod", side: BOTH, join = true)
}
#     fn init(_: &mut enshwick_loader_api::Load<'_>) -> Result<Self, enshwick_loader_api::Off> { Ok(Self) }
# }
```

`join = true` puts the mod in the mod set the server advertises, so a player
without it is warned before joining; see [The join check](concepts/join-check.md).
[`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
and
[`enshwick-building-materials`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-building-materials)
are content mods of this kind.

Every content call is made during `init`. The loader seals what mods
registered once they are initialised, and the game builds each resource the
first time it reads it.

## Content ids

Every piece of content a mod adds has a **content id**, `<mod>:<name>`:
the item, its recipe, the station template, the workshop, each text tag and
icon. The mod side is the mod's own id; the name is 1 to 128 of
`[a-z0-9_./-]`, so ids read like paths: `safes:safe`, `safes:safe.name`,
`survivors-per-altar:copy-staff/blacksmith`. Declare them as constants with
[`Id::new`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content.rs);
a malformed id fails the build:

```rust
use enshwick_loader_api::content::{Guid, Id};

const SAFE: Id = Id::new("safes:safe");
assert_eq!(SAFE.guid(), Guid::named("safes:safe"));
assert_eq!(SAFE.guid().to_string(), "f7eb124c-c132-5cf1-9c6c-e9e49ed3c896");
```

The game knows content by GUID, so the id turns into one: a version 5
(name-based, SHA-1) UUID of the id's UTF-8 bytes in Enshwick's namespace
[`NAMESPACE`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content.rs),
`e2185889-0e5e-4977-9edd-d6e7bcd1393f`. The same id gives the same GUID on
every machine, in every release and in any tool: Python's
`uuid.uuid5(uuid.UUID("e2185889-0e5e-4977-9edd-d6e7bcd1393f"), "safes:safe")`
prints the GUID above, and `cargo enshwick id safes:safe` prints it with
its game id. Saves hold these GUIDs and the ids derived from them, so a
content id is fixed for good once players have the content; to move content
to another id, keep its old one through a
[migration](migrations.md).

The content calls take an `Id` for what they create and declare it to the
loader, which turns the mod off when the id lies outside the mod's own
namespace or when its game id, a 32-bit hash of the GUID, is another
content id's, naming both ids and both mods:

```text
error [lanterns] off: content id lanterns:lamp of lanterns has the key 0x1a2b3c4d of content id torches:torch of torches; one of them needs another name
```

Content a mod serves itself with [`Load::serve`](content/resources.md) is
declared with `load.declare(id)`. References to existing content, a vanilla
item to clone or another mod's item for a recipe, stay plain
[`Guid`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content.rs)s:
`Guid::parse` for a vanilla GUID, `Guid::named("safes:safe")` for another
mod's content, without depending on that mod. Stat kinds and affixes have
`<mod>:<name>` ids too, and their keys come from the same GUIDs.

## Mods compose

Several mods can add items, recipes and workshops at once. The item
registry, the recipe registry, the workshop registry and the template list
are shared: each mod's change is a patch, and the loader runs the patches in
load order, each on the result of the one before. Two mods that each add an
item both end up in the registry.

What does not compose is two mods serving one new resource from different
bases, such as two mods adding an item under the same GUID cloned from
different vanilla items: the second is refused, and that mod is turned off.
[Served resources](content/resources.md) describes the rules in full.

## When a mod is removed

Removing a content mod leaves the world playable:

- items it added disappear from inventories, as the game drops item ids it
  does not know;
- a placed object stays. When its template is one the mod cloned, the loader
  serves the vanilla template it came from under the clone's GUID, so a
  station of a removed mod loads as the vanilla station it was cloned from.

For that fallback the loader records every template and item a mod clones,
with its vanilla source, in `mod-data/enshwick/clones.toml`. Keep the file:
the record outlives the mods. The loader logs how many cloned templates no
mod serves now and so read as their vanilla source. Items are recorded too,
though nothing reads them back yet.

The template fallback also applies when the game reads a cloned template
while the mod that serves it is installed but turned off.

# Content

A mod can add game content: items, recipes, crafting stations, texts, and
any other game resource. This part of the book covers the high-level calls
first and the served resources under them last:

- [Items](content/items.md): a new item cloned from a vanilla one;
- [Recipes](content/recipes.md): single recipes and derived copies of
  vanilla ones;
- [Workshops](content/workshops.md): a crafting station with a workshop of
  its own;
- [Loot containers](content/loot.md): reading the corpses and loot bags
  enemies leave;
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
# impl enshwick_api::Mod for MyMod {
fn manifest() -> &'static enshwick_api::abi::Manifest {
    enshwick_api::manifest!(id: "my-mod", side: BOTH, join = true)
}
#     fn init(_: &mut enshwick_api::Load<'_>) -> Result<Self, enshwick_api::Off> { Ok(Self) }
# }
```

`join = true` puts the mod in the mod set the server advertises, so a player
without it is warned before joining; see [The join check](concepts/join-check.md).
[`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
and
[`enshwick-building-yield`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-building-yield)
are content mods of this kind.

Every content call is made during `init`. The loader seals what mods
registered once they are initialised, and the game builds each resource the
first time it reads it.

## GUIDs are fixed forever

Every piece of content a mod adds has a GUID that the mod picks once: the
item, its recipe, the station template, the workshop, each text tag. Write
them as constants with [`Guid::parse`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/code.rs)
and never change them. Saves hold them: an item stack keeps the item's id,
which is derived from its GUID, and a placed station keeps its template's
GUID. A changed GUID orphans everything a player already has.

Generate a fresh random GUID for each new piece; never reuse one from another
mod or from the game.

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

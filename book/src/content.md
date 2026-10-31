# Content

A mod can add game content: items, recipes, crafting stations, texts, and
any other game resource.

## What this part covers

This part of the book covers the high-level calls
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
- [Texts](content/texts.md): names and descriptions per language;
- [Data packs](content/packs.md): all of the above from TOML files, in a
  folder with no code or inside a mod, and changes of vanilla values such as
  stack sizes.

[Inventories](inventories.md), which read and change what players and chests
hold, have a chapter of their own.

## Server and client alike

The game reads its resources on both sides: the server decides what a recipe
costs and what an item does, the client shows the item and its crafting menu.
A mod that adds content runs on both, and server and players must run the
same version of it. Its [manifest](concepts/loader-and-abi.md#the-manifest) says so:

```toml
[package.metadata.enshwick]
id = "you.my-mod"
side = "both"
join = true
```

`join = true` puts the mod in the mod set the server advertises, so a player
without it is warned before joining; see [The join check](concepts/join-check.md).
[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
and
[`enshwick-game-rates`](https://github.com/enshwick/mods/tree/main/game-rates)
are content mods of this kind.

Every content call is made during `init`. The loader seals what mods
registered once they are initialised, and the game builds each resource the
first time it reads it.

## Content ids

Every piece of content a mod adds has a **content id**, `<mod>:<name>`:
the item, its recipe, the station template, the workshop, each text tag and
icon. The mod side is the mod's own id; the name is 1 to 128 of
`[a-z0-9_./-]`, so ids read like paths: `you.lockers:locker`, `you.lockers:locker.name`,
`you.market-stalls:stall/blacksmith`.

A mod writes only the name.
[`id!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/ids.rs)
puts the mod's id in front, read from `[package.metadata.enshwick]` in the
crate's `Cargo.toml`, and `part` derives the ids that belong to one piece of
content from its name. Both are constants, and a malformed name fails the
build:

```rust
use enshwick::content::{Guid, Id, id};

// In the mod `you.lockers`, this is `you.lockers:locker`.
const LOCKER: Id = id!("locker");
const LOCKER_NAME: Id = LOCKER.part("name");

assert_eq!(LOCKER_NAME.to_string(), format!("{LOCKER}.name"));
assert_eq!(LOCKER.guid(), Guid::named(&LOCKER.to_string()));
```

`Id::new("you.lockers:locker")` takes an id in full, for tests and tools
that name content outside their own mod.

The game knows content by GUID, so the id turns into one: a version 5
(name-based, SHA-1) UUID of the id's UTF-8 bytes in Enshwick's namespace
[`NAMESPACE`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs),
`e2185889-0e5e-4977-9edd-d6e7bcd1393f`. The same id gives the same GUID on
every machine, in every release and in any tool: Python's
`uuid.uuid5(uuid.UUID("e2185889-0e5e-4977-9edd-d6e7bcd1393f"), "you.lockers:locker")`
prints `67842cb8-d551-542b-8765-fe1af9c28aca`, and `cargo enshwick id you.lockers:locker` prints it with
its game id. Saves hold these GUIDs and the ids derived from them, so a
content id is fixed for good once players have the content; to move content
to another id, keep its old one through a
[migration](migrations.md).

The content calls take an `Id` for what they create and declare it to the
loader, which turns the mod off when the id lies outside the mod's own
namespace, as an `Id::new` with another mod's id does, or when its game id, a 32-bit hash of the GUID, is another
content id's, naming both ids and both mods:

```text
error [lanterns] off: content id lanterns:lamp of lanterns has the key 0x1a2b3c4d of content id torches:torch of torches; one of them needs another name
```

Content a mod serves itself with [`Load::serve`](content/resources.md) is
declared with `load.declare(id)`. References to existing content, a vanilla
item to clone or another mod's item for a recipe, go through the mod's
`deps` module, `deps::<alias>::id(name)` and `deps::vanilla`, so its code
holds no GUID literal; [the manifest](concepts/loader-and-abi.md#the-manifest)
shows both. A vanilla name the game does not know turns the mod off, with the
names that look like it in the log. Another mod's content id passes only when
that mod [exports](concepts/loader-and-abi.md#what-other-mods-may-use) it.
`Guid::named("you.lockers:locker")` still names another mod's content
without depending on that mod, and without that check. Stat kinds and affixes have `<mod>:<name>` ids
too, and their keys come from the same GUIDs.

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

- items it added stay in inventories and chests as stand-ins. The game drops
  a stack whose item id it does not know, so for every item a mod cloned that
  no mod serves now the loader serves a stand-in under the item's own GUID: a
  copy of the vanilla item it was cloned from, named "Item of a removed mod".
  The stack keeps its id, count and data, and when the mod is installed again
  it serves its item and the stack is that item once more;
- a placed object stays. When its template is one the mod cloned, the loader
  serves the vanilla template it came from under the clone's GUID, so a
  station of a removed mod loads as the vanilla station it was cloned from.

For both the loader records every template and item a mod clones, with its
vanilla source, in `mod-data/enshwick/clones.toml`. Keep the file: the record
outlives the mods. The loader logs how many cloned templates read as their
vanilla source and how many items stand in. An item renamed by a mod's
[migration](migrations.md) gets no stand-in, since the game reads it under
its new GUID.

Both fallbacks also apply while the mod is installed but turned off. A
stand-in works as its vanilla item does, and the count of a stack is held to
the vanilla item's stack size when the world loads.

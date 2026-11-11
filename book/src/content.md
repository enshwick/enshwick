<!--
type: how-to
description: A mod adds game content with Enshwick: items, recipes, crafting stations, loot, texts and any other game resource, each under a content id of its own.
-->

# Content

A mod can add game content: items, recipes, crafting stations, texts, and
any other game resource.

## What this part covers

The chapters under Content cover the high-level calls. Texts and Data
packs sit under Everyday tasks, and Served resources, the bytes under
every call, under Reaching into the game:

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
  folder with no code or inside a mod. Packs also change vanilla values such
  as stack sizes;
- [What players do with items](content/item-actions.md): an item's
  behaviour when a player uses, trashes, equips, picks up or places it;
- [Rewards for actions](content/rewards.md): "when a player does this, give
  that", kept per world.

## How to build a content mod in steps

Start with data only, then add scripts where data is not enough:

1. A [data pack](content/packs.md): items, recipes, loot and rewards in
   TOML, with no code.
2. [World settings](content/packs.md#let-an-admin-tune-a-pack-per-world):
   numbers of `loot.toml` and `rewards.toml` that each world sets.
3. A [`main.luau`](luau.md) with functions the item's `on` table binds, for
   [what players do with items](content/item-actions.md).

The repository's
[`examples/you.shrine`](https://github.com/enshwick/enshwick/tree/main/examples/you.shrine)
takes these steps with one item.

[Inventories](inventories.md), which read and change what players and chests
hold, have a chapter of their own.

## Server and client alike

The game reads its resources on both sides. The server decides what a recipe
costs and what an item does. The client shows the item and its crafting menu.
A mod that adds content runs on both, and server and players must run the
same version of it. Its [manifest](concepts/loader-and-abi.md#the-manifest) says so:

```toml
[package.metadata.enshwick]
id = "you.my-mod"
side = "both"
```

`side = "both"` puts the mod in the mod set the server advertises. A player
without the mod is therefore kept out before joining and told which mod to
get. See [The join check](concepts/join-check.md).
[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
and
[`enshwick-game-rates`](https://github.com/enshwick/mods/tree/main/game-rates)
are content mods of this kind.

Every content call is made during `init`. The loader seals what mods
registered once they are initialised, and the game builds each resource the
first time it reads it.

## Content ids

Every piece of content a mod adds has a content id, `<mod>:<name>`, which
[how ids are namespaced](concepts/ids.md) defines with its GUID and limits. A
mod writes only the name, as `id!("locker")`. That covers the item, its
recipe, the station template, the workshop, each text tag and icon.

The content calls take an `Id` for what they create and declare it to the
loader. Content a mod serves itself with [`Load::serve`](content/resources.md)
is declared with `load.declare(id)`. An id outside the mod's namespace turns
the mod off, so an `Id::new` with another mod's id never creates content. A
vanilla name in `deps::vanilla` that the game does not know turns the mod off,
with similar names in the log.

## Renamed content

Content whose GUID changed keeps what saves hold under the old one through
[`Load::rename_content`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/load/content/renames.rs).
Declare every rename the mod ever made, oldest first, once in `init` and
before adding the content:

```rust
use enshwick::content::renames::Renames;
use enshwick::content::{Guid, Id, id};

const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
const LOCKER: Id = id!("locker");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.rename_content(Renames::new().rename(OLD_LOCKER, LOCKER.guid()))
}
```

The game then reads a placed object saved under the old GUID as the content
under the new one. An item the mod adds under the new GUID is served under
every old one too, so old stacks stay. A chain of renames reads as its
last GUID. Old stacks keep the old game id, so compare item ids through
`content::renames::current_id`. Nothing is written to a save or a file.

## Mods compose

Several mods can add items, recipes and workshops at once. The item
registry, the recipe registry, the workshop registry and the template list
are shared. Each mod's change is a patch. The loader runs the patches in load
order, each on the result of the one before. Two mods that each add an item
both end up in the registry.

What does not compose is two mods serving one new resource from different
bases. An example is two mods adding an item under the same GUID, cloned from
different vanilla items. The second is refused, and that mod is turned off.
[Served resources](content/resources.md) describes the rules in full.

## When a mod is removed

Removing a content mod leaves the world playable:

- items it added stay in inventories and chests as stand-ins. The game drops
  a stack whose item id it does not know. For every item a mod cloned that no
  mod serves now, the loader therefore serves a stand-in under the item's own
  GUID. The stand-in is a copy of the vanilla item it was cloned from, named
  "Item of a removed mod". The stack keeps its id, count and data. When the
  mod is installed again, it serves its item and the stack is that item once
  more;
- a placed object stays. When its template is one the mod cloned, the loader
  serves the vanilla template it came from under the clone's GUID. A station
  of a removed mod therefore loads as the vanilla station it was cloned from.

For both the loader records every template and item a mod clones, with its
vanilla source, in `mods/enshwick/clones.toml`. Keep the file: the record
outlives the mods. The loader logs how many cloned templates read as their
vanilla source and how many items stand in. An item a mod
[renamed](#renamed-content) gets no stand-in, since the game reads it under
its new GUID.

Both fallbacks also apply while the mod is installed but turned off. A
stand-in works as its vanilla item does. When the world loads, the count of a
stack is held to the vanilla item's stack size.

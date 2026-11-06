<!--
type: how-to
description: Mods take vanilla items, workshops and loot tables out of play, and serve a vanilla item with another item's data.
-->

# Removing and re-keying vanilla content

A mod takes vanilla items, workshops and loot tables out of play while it
runs, and serves a vanilla item with another item's data. Nothing leaves the
game's data and nothing is written to a save.

## Why does nothing leave the data?

The server clears every held stack whose item the registry lacks when an
inventory loads, without a log line, and the next save keeps that loss. So
every call here keeps the vanilla entry registered: a world opens as vanilla
when the mod is removed. On a server copy of server 399, a world of 139 held
stacks kept all of them through each call below.

## Take an item out of play

`load.remove_item(item)` keeps the item registered and makes it unreachable:

- no recipe makes it, and no crafting or build menu lists it;
- no container's fill table or starting inventory gives it;
- the random loot tables lose its entries.

The stacks players hold keep it. A mod that wants held copies gone takes them
out with the [inventory](../inventories.md) calls. Mods' loot changes chain in
load order, so a later mod that gives the item a count gives it back.

```rust
use enshwick::content::vanilla;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.remove_item(vanilla::item("Tool_Torch01")?)?;
    load.remove_workshop(vanilla::item("Craftingstation_Workbench")?)?;
    load.rekey_item(vanilla::item("Material_T1_Resin")?, vanilla::item("Material_T1_Wood_Logs")?)?;
    Ok(())
}
```

## Take a workshop out of play

`load.remove_workshop(station)` names the station by the item that places it.
No recipe is made at its workshop, and no recipe builds the station. Placed
stations stay in the world: each is an entity of the station's template,
which the mod leaves alone.

## Empty a loot table

`load.remove_loot_table(table)` serves a random loot table or a container's
fill table that gives nothing. Loot players hold is plain stacks and stays.
A table the game lacks returns `Err(Off)`.

## Serve one item with another's data

`load.rekey_item(old, new)` serves the vanilla item `old` with the data of
the vanilla item `new`, under `old`'s own id. Saves, recipes and loot that
name `old` get `new`'s look and use. On a server copy of server 399, wood
logs re-keyed to an arrow read as the arrow, stack size 500 and category
ammunition, and every held stack of logs stayed.

- The stack size is at least `old`'s: the game cuts a held stack above its
  item's stack size and saves the cut.
- Saves keep `old`'s id, so a world opens as vanilla without the mod.
- An item with an entity of its own (`ItemFlag::HasPIDE`), such as one with
  durability, re-keys only to another such item; otherwise the call returns
  `Err(Off)`, since the game would make or clear that entity in every held
  stack.

## Recipes

A recipe goes out of play with
[`recipes::disable`](disabled-recipes.md), which a world's recipe file can
also do.

These calls need the `typed` feature.

## From Luau

The same calls are `enshwick.content.remove_item(item)`,
`remove_workshop(station)`, `remove_loot_table(table)` and
`rekey_item(old, new)`, each by GUID, at the top level of `main.luau`.

## Who needs the mod

The server decides what drops and what a recipe makes. A client draws item
names, icons and menus from its own data, so a mod that re-keys an item runs
on both sides.

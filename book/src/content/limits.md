# Vanilla value changes from a mod

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](../stability.md)).

A Rust mod changes vanilla values, such as an item's stack size or a chest's
slots, with the same [changes](packs.md#changes-of-vanilla-values) data packs
make.

## Changes at init

A change takes the same fields, ranges and selections as a pack's, merged by
the same rule with every pack and mod
([Several packs on one value](packs.md#several-packs-on-one-value)).

[`Load::change`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/changes.rs)
declares a change during `init`. With `Load::change_items` and
`Load::change_recipes` it makes the selections a pack makes. The loader reads
the selection from the game's data and merges one change per item in the
mod's place in load order, as it does for a pack:

```rust
use enshwick::content::categories::ItemCategoryMask;
use enshwick::content::changes::{Field, Items, ItemsChange, Value};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let materials = Items::Category(ItemCategoryMask::MATERIALS);
    load.change_items(&ItemsChange::new(materials, Field::MaxStackSize, Value::Times(4.0)), "stacks")
}
```

A stack size reaches only items that already stack and keep no state of
their own, such as durability; weapons and armour stay at one. Overlapping
selections of two mods report one conflict line per pair, with the count of
items it covers.

## Declare at init, set later

A change declared with `Load::declare_change` takes its value any time after
`init`, such as from a setting that differs per world.

```rust
use enshwick::content::categories::ItemCategoryMask;
use enshwick::content::changes::{ChangeHandle, Field, Items, Target, Value};

fn init(load: &mut enshwick::Load<'_>) -> Result<(ChangeHandle, ChangeHandle), enshwick::Off> {
    let materials = Target::Items(Items::Category(ItemCategoryMask::MATERIALS));
    let stacks = load.declare_change(materials, Field::MaxStackSize, "stacks")?;
    let chests = load.declare_change(Target::Items(Items::Containers), Field::Slots, "chests")?;
    stacks.set(Value::Times(4.0)).map_err(|why| enshwick::Off::new(why.to_string()))?;
    chests.set(Value::Set(48.0)).map_err(|why| enshwick::Off::new(why.to_string()))?;
    Ok((stacks, chests))
}
```

- `Target::Entry` names one vanilla item; `Target::Items` picks every item
  of some categories, every item that stacks, or every item that places a
  container, read from the game's data.
- Until `set`, the change leaves every value as the other mods make it.
  `reset` takes the mod's value out again.
- Each `set` merges every change again and builds the items, chest
  templates, recipes and loot tables it reaches anew in place. It
  allocates: call it when a setting changes, not every frame.
- Values take the same ranges as data packs: a stack size from 1 to 65535,
  slots from 8 to 128 in steps of 8.

## Which games read it

The server decides. Its game cuts every stack above an item's stack size
and sizes every chest it places. A player's game uses its own stack sizes
for the split and merge it offers and for what it shows, so every side
needs the same value. A change of an item or a recipe therefore needs a
mod with `join: true`, which sets the value from a synced setting
(`#[config(startup, sync)]`, see [join values](../settings.md#join-values)): a player
joining a server takes the server's value. Loot changes reach the server
alone and need no join.

## Chests

A chest keeps the size it was placed with, which the save holds. A new
size reaches chests placed after the change; a chest a player upgrades takes
the new size of the chest it becomes. Slots only grow: a chest a player
upgrades moves every item it holds.

## Stack sizes and saved worlds

The game cuts every saved stack above an item's stack size when a world
loads, before any per-world setting applies, and deletes the rest. Two things
keep a saved world's stacks.

**A pack's hold.** The first time a world saved with a pack that raised a
stack size loads, the pack sets a [hold](../state.md#holds) on that world.
If the world is opened again without the pack, the game stops before the
world loads and the log names the pack and items; other worlds open without
the pack as usual. When several packs raise one item, each of them holds. A
pack that only lowers stack sizes sets no hold.

```text
error you.stacks holds this game (it lets Material_T1_Wood_Logs and 12 more items stack past the game's own size (20 to 999); ..., in world mygame-1a2b3c4d) and the mod does not run: the game stops before the world loads
```

**The kept sizes.** The loader keeps every stack size above the game's that
was in force when a world was saved, in `mod-data/enshwick/kept-stacks.toml`,
and serves at least that size from the start, for every mod and pack and
without them:

```text
info  stacks of 3f4093ae-75ed-4e5d-bd0b-784ba83b88f5 stay at 999, not 100: a saved world may hold stacks that large
```

- A lower value from a mod or pack takes effect only above the kept size.
  To lower it, split every larger stack, then delete the item's line in the
  file while the game is stopped.
- Removing the mod or pack that raised a size keeps the size too: the loader
  still serves it.
- While any size is kept, the loader [holds](../state.md#holds) the game:
  on a game build it does not know, which it would leave vanilla, the game
  stops before a world loads and cuts nothing.

The two meet when a raising pack is removed: its hold stops the game at that
world until its `hold.txt` is deleted, and after that the kept size still
keeps the world's stacks whole.

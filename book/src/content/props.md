<!--
type: how-to
description: A prop is a placeable object. Mods add prop types copied from vanilla props, with a recipe in the build menu, and crafting props that recipes require.
-->

# Prop types

A prop is an object a player places with the build tool, such as a chest. A
mod adds a prop type copied from a vanilla prop: an item of its own places a
template of its own.

## A prop of the mod's own

`load.add_prop(NewProp)` copies the vanilla prop whose item is
`item.like`:

- the item, as [`add_item`](items.md) copies it, with the name, description,
  icon and lines `item` gives;
- the [template](templates.md) the vanilla item places, as `<id>.template`,
  with the `template` changes applied in order; the mod's item places this
  copy;
- with `recipe_cost`, the vanilla prop's recipe as `<id>.recipe`, its
  ingredient counts times the cost. The build menu lists the prop after the
  vanilla one, unlocked with it.

It returns a `PropId`: the item, which `world::Prop::item` reports for every
placed one, and the template.

The prop's other ids derive from the item's id, so the item's id is a plain
id, never one made with `Id::part`. A world keeps the template GUID of each
placed prop: the ids never change.

## A crafting prop

A crafting prop stands near a workshop and lets it make recipes that require
it, as the vanilla mortar does. `crafting_prop` makes the new prop one:

- the template is marked as the crafting prop `<id>.crafting-prop`;
- the game's workshop registry lists it;
- each recipe in `recipes` requires it beside the props it requires already.

A recipe named there is a vanilla one or one
the mod added before the call.

The game keeps the crafting props of a base in one mask of 64 bits. Vanilla
and every mod share it, and the game's own props take 51 of the bits. The loader
gives each new crafting prop the next free bit, in load order, and refuses one
past the last, which turns its mod off. [Load order](../concepts/lifecycle.md#in-what-order-do-mods-load)
follows mod ids and dependencies, so the same mods give the same bits. The server sends clients
the bits of a base's crafting props, so only a mod of side both adds a
crafting prop. A client takes the same bits when it runs the world's mods
and no other mod with a crafting prop.

```rust
use enshwick::content::{Guid, id};
use enshwick::content::items::NewItem;
use enshwick::content::props::{CraftingPropSetup, NewProp};

fn init(load: &mut enshwick::Load<'_>, chest: Guid, mortar: Guid, potion: Guid) -> Result<(), enshwick::Off> {
    let crate_prop = load.add_prop(NewProp {
        recipe_cost: Some(2),
        ..NewProp::new(NewItem::new(id!("crate"), chest))
    })?;
    enshwick::info!("crates place {}", crate_prop.template.guid());
    load.add_prop(NewProp {
        crafting_prop: Some(CraftingPropSetup { recipes: vec![potion] }),
        ..NewProp::new(NewItem::new(id!("alembic"), mortar))
    })?;
    Ok(())
}
```

A prop is read on the server and the client, so a mod that adds one runs on
both sides.

## From Luau

The same call is `enshwick.content.add_prop{...}`, at the top level of
`main.luau`, with the same fields. Its `item` is the table `add_item` takes.

```luau
local chest = "<the GUID of the game's chest item>"
local crate = enshwick.content.add_prop({
    item = { id = "you.props:crate", like = chest },
    recipe_cost = 2,
})
enshwick.log.info(`crates place {crate.template}`)
```

## When the mod is removed

The loader serves the vanilla template under the copy's GUID, so placed props
load as the vanilla one: see
[Content](../content.md#when-a-mod-is-removed).

<!--
type: how-to
description: An entity template is what a placed object or a creature is made of. Mods copy a vanilla template under their own id and change a template's components.
-->

# Entity templates

An entity template is what a placed object or a creature is made of. A mod
copies a game template under its own content id and changes the components
of a copy or of a vanilla template.

## What a template holds

A template has a family, such as `Prop`, and a list of components. Enemies,
props, altars, animals and survivors are all templates. Changes happen in
memory: nothing is written to a save or the game's archive.

## A copy with changed components

`load.add_template(NewTemplate)` copies the vanilla template `like`. The copy
takes the id `id`, and the call applies `changes` in order. The copy keeps the family, model and
proxy model of the template it copies, so it looks like it. It is listed in
the game's template collection and returns a `TemplateId`, the GUID saves keep
for every object or creature made from it.

`load.change_template(guid, changes)` changes a vanilla template. Mods that
change one template change it in load order, each on the one before's result.

A template is read on the server and the client, so a mod that adds one runs
on both sides.

## What a change does

A `ComponentChange` names its component by its type and does one of three
things:

| Change | With the `typed` feature | Without it |
| --- | --- | --- |
| Set the component whole; add it when the template lacks one | `ComponentChange::set(&value)` | `set_value(id, Value)` |
| Edit some of its fields | `ComponentChange::edit(\|c: &mut T\| ...)` | `edit_value(id, ...)`, `edit_bytes(id, ...)` |
| Remove it | `ComponentChange::remove::<T>()` | `remove_named(id)` |

With the `typed` feature the component is one of the
[generated types](../game-types.md). The loader checks at `init` that the
running build's type still has its shape. Without it, a change names the
component by enshwick's id for its type and works on the codec's
[values](../resource-values.md) or on the component's bytes.

A set adds a component only when templates of the same family carry its type
somewhere in the game's data. The game's bosses are `Npc` templates. So an
`Npc` template takes a `Boss` component. It takes no crafting component.
These return `Err(Off)` from the call:

- any other set;
- a remove or edit of a component the template lacks;
- a `like` that is no vanilla template.

```rust
use enshwick::content::{Guid, id};
use enshwick::content::templates::{ComponentChange, NewTemplate};
use enshwick::typed::FactionType;
use enshwick::typed::ecs::{Aggro, Faction};

fn init(load: &mut enshwick::Load<'_>, wolf: Guid) -> Result<(), enshwick::Off> {
    let vukah_wolf = load.add_template(NewTemplate {
        changes: vec![
            ComponentChange::edit(|faction: &mut Faction| faction.faction = FactionType::Vukah),
            ComponentChange::remove::<Aggro>(),
        ],
        ..NewTemplate::new(id!("vukah-wolf"), wolf)
    })?;
    enshwick::info!("vukah wolves are {}", vukah_wolf.guid());
    load.change_template(wolf, vec![ComponentChange::remove::<Aggro>()])
}
```

## From Luau

The same calls are `enshwick.content.add_template{...}` and
`enshwick.content.change_template(guid, {...})`. They run at the top level of
`main.luau`. A change is a table: `component` is the generated type's path in
the typed API, such as `ecs.Faction`. With `set` the table is the whole
component by its Rust field names, `base` included. With `edit` it names only
the fields it changes. With neither, the component is removed.

```luau
local wolf = "<the GUID of one of the game's creature templates>"
local vukah_wolf = enshwick.content.add_template({
    id = "you.wolves:vukah-wolf",
    like = wolf,
    changes = {
        { component = "ecs.Faction", edit = { faction = "Vukah" } },
        { component = "ecs.Aggro" },
    },
})
enshwick.content.change_template(wolf, { { component = "ecs.Aggro" } })
```

## When the mod is removed

A world keeps the GUID of each object or creature made from a copy. When the
mod is removed, the loader serves the vanilla template under the copy's GUID,
so they load as the vanilla one: see
[Content](../content.md#when-a-mod-is-removed). A changed vanilla template is
vanilla again at the next start.

[Workshops](workshops.md) and [item meters](items.md) are templates copied
this way.

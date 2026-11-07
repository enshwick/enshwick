<!--
type: how-to
description: A workshop is what a crafting station crafts at. Load::add_workshop clones a vanilla station into a new one with a workshop of its own for a mod's recipes.
-->

# Workshops

A workshop is what a crafting station crafts at.
[`Load::add_workshop`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/workshops.rs)
clones a vanilla station into a new one with a workshop of its own, and
returns its
[`WorkshopId`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/workshops.rs)
for your recipes.

## A new station

Recipes name a workshop, and a placed station opens the crafting menu of its
workshop.

```rust
use enshwick::content::{Id, id};
use enshwick::content::recipes::Derived;
use enshwick::content::workshops::{NewWorkshop, stations};

const BENCH: Id = id!("bulk-bench");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let new = NewWorkshop {
        named: true,
        described: true,
        recipe_cost: Some(1),
        debug_name: "Craftingstation_BulkWorkbench".into(),
        ..NewWorkshop::new(BENCH, stations::WORKBENCH)
    };
    let texts = load.texts_from(r#"
[en]
"bench.name" = "Bulk Workbench"
"bench.description" = "A workbench for building in bulk."
"#)?;
    load.add_texts(&texts, &[
        ("bench.name", new.name()),
        ("bench.description", new.description()),
    ])?;
    let bench = load.add_workshop(new)?;
    load.derive_recipes("my-mod", move |recipe| {
        recipe
            .debug_name()
            .starts_with("Block_")
            .then_some(Derived { output: 2, cost: 1, workshop: Some(bench) })
    })
}
```

```luau
local BENCH = "you.bench:bulk-bench"

local texts = enshwick.i18n.texts_from([[
[en]
"bench.name" = "Bulk Workbench"
"bench.description" = "A workbench for building in bulk."
]])
enshwick.i18n.add_texts(texts, {
    { "bench.name", `{BENCH}.name` },
    { "bench.description", `{BENCH}.description` },
})
local bench = enshwick.content.add_workshop({
    like = enshwick.content.station("Craftingstation_Workbench"),
    id = BENCH,
    named = true,
    described = true,
    recipe_cost = 1,
    debug_name = "Craftingstation_BulkWorkbench",
})
enshwick.content.derive_recipes("my-mod", function(recipe)
    if string.sub(recipe.debug_name, 1, 6) == "Block_" then
        return { output = 2, cost = 1, workshop = bench }
    end
    return nil
end)
```

In Luau, `enshwick.content.station` reads a vanilla station by its item's
debug name, and `add_workshop` returns the workshop's GUID.

The mod writes one name. The station item is `BENCH`, and every other id
the station needs derives from it:

| Id | What it names |
| --- | --- |
| `<mod>:bulk-bench` | the station item |
| `<mod>:bulk-bench.template` | the station's template, `new.template()` |
| `<mod>:bulk-bench.workshop` | the workshop, `new.workshop()` |
| `<mod>:bulk-bench.recipe` | the recipe that makes the station, `new.recipe()` |
| `<mod>:bulk-bench.name` | the tag of the station's name, `new.name()` |
| `<mod>:bulk-bench.description` | the tag of the item's description, `new.description()` |

## What it adds

`add_workshop` serves five resources, a sixth on the client, and with
`recipe_cost` adds a recipe:

1. the station's template, an [entity template](templates.md) copied from the
   vanilla one. Its crafting components name the new workshop. Two are
   `Crafting` and `CraftingInteraction`. The third is
   `ClientCraftingInteraction`;
2. the template collection with the new template listed;
3. the station item, a clone of the vanilla station item that places the new
   template;
4. the item registry with the new item listed;
5. the workshop registry with a copy of the vanilla station's entry, naming
   the new workshop, the new item and, with `named`, the new name;
6. on the client, the UI bundle with a crafting menu for the new workshop.
   The menu has one group with one empty set, where its recipes go
   ([The crafting menu](recipes.md#the-crafting-menu));
7. with `recipe_cost`, a copy of the vanilla station's recipe that makes
   the new station. The copy is made at the workshop where the vanilla station
   is crafted, with the recipe's ingredient counts times the cost.

The new workshop starts with no recipes. Only recipes you send there are
made at it.

## `NewWorkshop`

| Field | Meaning |
| --- | --- |
| `like` | The vanilla station cloned, a [`Station`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/workshops.rs): its item and the template the item places. |
| `id` | The new station item's content id, which every other id derives from. |
| `named` | Whether the station takes the name `<id>.name`, which also names the workshop; `false` keeps the vanilla names. |
| `described` | Whether the station item takes the description `<id>.description`; `false` keeps the vanilla one. |
| `recipe_cost` | With a cost, the recipe `<id>.recipe` makes the station from the vanilla station's ingredients times the cost; `None` adds no recipe. |
| `debug_name` | The item's name in the registry's debug list; the id unless set. |

`NewWorkshop::new(id, like)` makes one with the vanilla names and no
recipe; set the other fields with struct update syntax. Give the name and
description their texts with [`add_texts`](texts.md). Their tags are
`new.name()` and `new.description()`.

[`stations::WORKBENCH`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/workshops.rs)
is the Workbench. Any other crafting station comes from the game's data.
[`Vanilla::game()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/vanilla.rs)
finds an item by its debug name. `station(item)` reads the template the item
places, after two checks:

- the workshop registry has a workshop of the item's own that is no
  survivor's;
- the template has all three crafting components.

`workshop(item)` is the station's own workshop, for recipes made at the
vanilla station.

```rust
use enshwick::Off;
use enshwick::content::vanilla::Vanilla;

fn forge() -> Result<enshwick::content::workshops::Station, Off> {
    let vanilla = Vanilla::game().ok_or_else(|| Off::new("no game data"))?;
    let item = vanilla.item("Prop_Production_T1_Forge")?;
    Ok(vanilla.station(item)?)
}
```

On build 1269579 that covers over thirty crafting stations, the Forge,
Kiln, Smelter, Loom, Alchemy Station, Oven and Kitchen among them.

Every [content id](../content.md#content-ids) is the mod's own and fixed for
good. That is because saves keep the template GUID of each placed station and
the item id of each stack. `add_workshop` declares the item's, the template's and the
workshop's ids. An id that is a part already, or too long to take
`.description`, turns the mod off.

## Recipes at the new workshop

Send recipes to the workshop with its `WorkshopId`:

- `NewRecipe { workshop: Some(id), .. }` for a [single recipe](recipes.md#a-single-recipe);
- `Derived { workshop: Some(id), .. }` for [derived copies](recipes.md#derived-recipes).

A `WorkshopId` is only a GUID with the game's id beside it. `add_workshop`
returns one, and
[`WorkshopId::new`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/workshops.rs)
names any workshop, a vanilla one included:

```rust
use enshwick::content::Guid;
use enshwick::content::workshops::WorkshopId;

let workbench = WorkshopId::new(Guid::parse("965d3192-0287-49a2-ad51-a716139571a6"));
assert_eq!(workbench.id(), 1_302_892_403);
assert_eq!(workbench.id(), workbench.guid().fnv1a());
```

Copies made at the new workshop count against its own room, not against the
vanilla workshop's; see [The workshop limit](recipes.md#the-workshop-limit).

## Errors and removal

`add_workshop` returns `Err(Off)` when a layout is not reflected or the
vanilla station's template lacks a crafting component. A vanilla station
whose item does not place `like.template` is found out when the game first
reads it, and turns the mod off then.

When the mod is removed, placed stations load as the vanilla station they
were cloned from: see [Content](../content.md#when-a-mod-is-removed). The
station items players hold stay as stand-ins, like any item of a removed mod.

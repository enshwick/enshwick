# Workshops

A workshop is what a crafting station crafts at: recipes name a workshop,
and a placed station opens the crafting menu of its workshop.
[`Load::add_workshop`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/workshops.rs)
clones a vanilla station into a new one with a workshop of its own, and
returns its
[`WorkshopId`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/workshops.rs)
for your recipes.

```rust
use enshwick_loader_api::content::Id;
use enshwick_loader_api::content::recipes::Derived;
use enshwick_loader_api::content::workshops::{NewWorkshop, StationRecipe, stations};

const BENCH: Id = Id::new("my-mod:bulk-bench");
const BENCH_TEMPLATE: Id = Id::new("my-mod:bulk-bench.template");
const BENCH_WORKSHOP: Id = Id::new("my-mod:bulk-bench.workshop");
const BENCH_RECIPE: Id = Id::new("my-mod:bulk-bench.recipe");
const BENCH_NAME: Id = Id::new("my-mod:bulk-bench.name");
const BENCH_DESCRIPTION: Id = Id::new("my-mod:bulk-bench.description");

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let texts = load.texts(r#"
[en]
"bench.name" = "Bulk Workbench"
"bench.description" = "A workbench for building in bulk."
"#)?;
    load.add_texts(&texts, &[
        ("bench.name", BENCH_NAME),
        ("bench.description", BENCH_DESCRIPTION),
    ])?;
    let bench = load.add_workshop(NewWorkshop {
        like: stations::WORKBENCH,
        item: BENCH,
        template: BENCH_TEMPLATE,
        workshop: BENCH_WORKSHOP,
        name: Some(BENCH_NAME.guid()),
        description: Some(BENCH_DESCRIPTION.guid()),
        station_recipe: Some(StationRecipe { id: BENCH_RECIPE, cost: 1 }),
        debug_name: "Craftingstation_BulkWorkbench".into(),
    })?;
    load.derive_recipes("my-mod", move |recipe| {
        recipe
            .debug_name()
            .starts_with("Block_")
            .then_some(Derived { output: 2, cost: 1, workshop: Some(bench) })
    })
}
```

## What it adds

`add_workshop` serves five resources, and with `station_recipe` adds a
recipe:

1. the station's template, a copy of the vanilla one whose crafting
   components (`Crafting`, `CraftingInteraction`,
   `ClientCraftingInteraction`) name the new workshop;
2. the template collection with the new template listed;
3. the station item, a clone of the vanilla station item that places the new
   template;
4. the item registry with the new item listed;
5. the workshop registry with a copy of the vanilla station's entry, naming
   the new workshop, the new item and, with `name`, the new name;
6. with `station_recipe`, a copy of the vanilla station's recipe that makes
   the new station, at the workshop where the vanilla station is crafted,
   its ingredient counts times `cost`.

The new workshop starts with no recipes. Only recipes you send there are
made at it.

## `NewWorkshop`

| Field | Meaning |
| --- | --- |
| `like` | The vanilla station cloned, a [`Station`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/workshops.rs): its item and the template the item places. |
| `item` | The new station item's content id. |
| `template` | The new station's template's content id. |
| `workshop` | The new workshop's content id. |
| `name` | The tag of the station's name, which also names the workshop; `None` keeps the vanilla names. |
| `description` | The tag of the station item's description; `None` keeps the vanilla one. |
| `station_recipe` | How the station is crafted, a [`StationRecipe`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/workshops.rs) with the recipe's content id and a cost factor; `None` adds no recipe. |
| `debug_name` | The item's name in the registry's debug list. |

[`stations::WORKBENCH`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/workshops.rs)
is the Workbench. Any other crafting station comes from the game's data:
[`Vanilla::game()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/vanilla.rs)
finds an item by its debug name and `station(item)` reads the template the
item places, after checking that the workshop registry has a workshop of the
item's own that is no survivor's and that the template has all three
crafting components. `workshop(item)` is the station's own workshop, for
recipes made at the vanilla station.

```rust
use enshwick_loader_api::Off;
use enshwick_loader_api::content::vanilla::Vanilla;

fn forge() -> Result<enshwick_loader_api::content::workshops::Station, Off> {
    let vanilla = Vanilla::game().ok_or_else(|| Off::new("no game data"))?;
    let item = vanilla
        .item("Prop_Production_T1_Forge")
        .map_err(|why| Off::new(why.to_string()))?;
    vanilla.station(item).map_err(|why| Off::new(why.to_string()))
}
```

On build 1269579 that covers over thirty crafting stations, the Forge,
Kiln, Smelter, Loom, Alchemy Station, Oven and Kitchen among them.

Every [content id](../content.md#content-ids) is the mod's own and fixed for
good: saves keep the template GUID of each placed station and the item id of
each stack. `add_workshop` declares the item's, the template's and the
workshop's ids.

## Recipes at the new workshop

Send recipes to the workshop with its `WorkshopId`:

- `NewRecipe { workshop: Some(id), .. }` for a [single recipe](recipes.md#a-single-recipe);
- `Derived { workshop: Some(id), .. }` for [derived copies](recipes.md#derived-recipes).

A `WorkshopId` is only a GUID with the game's id beside it. `add_workshop`
returns one, and
[`WorkshopId::new`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/workshops.rs)
names any workshop, a vanilla one included:

```rust
use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::workshops::WorkshopId;

let workbench = WorkshopId::new(Guid::parse("965d3192-0287-49a2-ad51-a716139571a6"));
assert_eq!(workbench.id(), 1_302_892_403);
assert_eq!(workbench.id(), workbench.guid().id());
```

Copies made at the new workshop count against its own room, not against the
vanilla workshop's; see [The workshop limit](recipes.md#the-workshop-limit).

## Errors and removal

`add_workshop` returns `Err(Off)` when a layout is not reflected. A vanilla station whose template lacks a crafting component,
or whose item does not place `like.template`, is found out when the game
first reads them, and turns the mod off then.

When the mod is removed, placed stations load as the vanilla station they
were cloned from: see [Content](../content.md#when-a-mod-is-removed). The
station items players hold stay as stand-ins, like any item of a removed mod.

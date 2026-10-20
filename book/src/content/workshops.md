# Workshops

A workshop is what a crafting station crafts at: recipes name a workshop,
and a placed station opens the crafting menu of its workshop.
[`Load::add_workshop`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/workshops.rs)
clones a vanilla station into a new one with a workshop of its own, and
returns its
[`WorkshopId`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/workshops.rs)
for your recipes.

```rust
use enshwick_api::content::Guid;
use enshwick_api::content::recipes::Derived;
use enshwick_api::content::workshops::{NewWorkshop, StationRecipe, stations};

const BENCH: Guid = Guid::parse("0851e93d-8883-42e4-a807-c30fd644e7c3");
const BENCH_TEMPLATE: Guid = Guid::parse("77ff6155-1365-4a4a-8987-507a1bdd32a2");
const BENCH_WORKSHOP: Guid = Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6");
const BENCH_RECIPE: Guid = Guid::parse("94cf68a3-11e0-4c05-a793-526f7e1d7693");
const BENCH_NAME: Guid = Guid::parse("41b222ea-cefa-4774-a040-c43aa9f0b227");
const BENCH_DESCRIPTION: Guid = Guid::parse("6bc87387-936a-40a5-8e62-5aa5dbf933eb");

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
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
        name: Some(BENCH_NAME),
        description: Some(BENCH_DESCRIPTION),
        station_recipe: Some(StationRecipe { guid: BENCH_RECIPE, cost: 1 }),
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
| `like` | The vanilla station cloned, a [`Station`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/workshops.rs): its item and the template the item places. |
| `item` | The new station item's GUID. |
| `template` | The new station's template GUID. |
| `workshop` | The new workshop's GUID. |
| `name` | The tag of the station's name, which also names the workshop; `None` keeps the vanilla names. |
| `description` | The tag of the station item's description; `None` keeps the vanilla one. |
| `station_recipe` | How the station is crafted, a [`StationRecipe`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/workshops.rs) with the recipe's GUID and a cost factor; `None` adds no recipe. |
| `debug_name` | The item's name in the registry's debug list. |

[`stations`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/workshops.rs)
lists the vanilla stations known to clone; it holds `WORKBENCH` today. A
`Station` with other GUIDs works if the item places that template, the
template has all three crafting components and the workshop registry has an
entry for the item.

Every GUID is the mod's own and fixed for good: saves keep the template GUID
of each placed station and the item id of each stack.

## Recipes at the new workshop

Send recipes to the workshop with its `WorkshopId`:

- `NewRecipe { workshop: Some(id), .. }` for a [single recipe](recipes.md#a-single-recipe);
- `Derived { workshop: Some(id), .. }` for [derived copies](recipes.md#derived-recipes).

A `WorkshopId` is only a GUID with the game's id beside it. `add_workshop`
returns one, and
[`WorkshopId::new`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/load.rs)
names any workshop, a vanilla one included:

```rust
use enshwick_api::content::Guid;
use enshwick_api::content::workshops::WorkshopId;

let workbench = WorkshopId::new(Guid::parse("965d3192-0287-49a2-ad51-a716139571a6"));
assert_eq!(workbench.id(), 1_302_892_403);
assert_eq!(workbench.id(), workbench.guid().id());
```

Copies made at the new workshop count against its own room, not against the
vanilla workshop's; see [The workshop limit](recipes.md#the-workshop-limit).

## Errors and removal

`add_workshop` returns `Err(Off)` when the loader is too old or a layout is
not reflected. A vanilla station whose template lacks a crafting component,
or whose item does not place `like.template`, is found out when the game
first reads them, and turns the mod off then.

When the mod is removed, placed stations load as the vanilla station they
were cloned from: see [Content](../content.md#when-a-mod-is-removed). The
station items players hold disappear, like any item of a removed mod.

[`enshwick-building-yield`](https://github.com/themaiby/enshwick/blob/main/mods/enshwick-building-yield/src/lib.rs)
adds its Bulk Workbench this way.

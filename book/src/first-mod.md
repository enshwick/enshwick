<!--
type: tutorial
description: Build a mod that adds a travel chest to Enshrouded, step by step. It covers the item, its name in two languages, its recipe and a setting for its cost.
-->

# Your first real mod

You build a mod that adds a travel chest, a new item. The mod gives it its
own name, a recipe and a setting for its cost. Each step shows the Rust
code and the same step in Luau.

## Start the mod

Every player crafts the chest and sees its name, so the mod runs on both
sides:

```bash
cargo enshwick new you.travel-chest --side both
```

The recipe calls are [unstable](stability.md). Open `Cargo.toml`. Under
`[dependencies]`, replace the line `enshwick.workspace = true` with:

```toml
[dependencies]
enshwick = { workspace = true, features = ["unstable"] }
```

A Luau mod starts with `cargo enshwick new --luau you.travel-chest --side both`
and keeps every step in `main.luau`.

## Name the chest

The chest's name and description go into `src/texts.toml`, beside the
texts `new` wrote. Every key needs English and Ukrainian, or the mod does
not build:

```toml
[en]
"chest.name" = "Travel chest"
"chest.description" = "A storage box that packs small."

[uk]
"chest.name" = "Дорожня скриня"
"chest.description" = "Ящик для речей, що мало важить."
```

A Luau mod keeps the same file as `texts.toml` beside `main.luau`.
[Texts](content/texts.md) has the languages and the rules.

## Add the item

The chest is a clone of the vanilla storage box under a
[content id](concepts/ids.md) of its own. It looks and works like the
box and takes its name from the texts:

```rust
use enshwick::content::items::NewItem;
use enshwick::content::{Id, id, vanilla};

const CHEST: Id = id!("chest");
const CHEST_NAME: Id = CHEST.part("name");
const CHEST_DESCRIPTION: Id = CHEST.part("description");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let texts = load.texts()?;
    load.add_texts(&texts, &[
        ("chest.name", CHEST_NAME),
        ("chest.description", CHEST_DESCRIPTION),
    ])?;
    load.add_item(NewItem {
        name: Some(CHEST_NAME.guid()),
        description: Some(CHEST_DESCRIPTION.guid()),
        ..NewItem::new(CHEST, vanilla::item("Prop_Storage_24_T1_Box")?)
    })
}
```

```luau
local CHEST = "you.travel-chest:chest"

local texts = enshwick.i18n.texts()
enshwick.i18n.add_texts(texts, {
    { "chest.name", `{CHEST}.name` },
    { "chest.description", `{CHEST}.description` },
})
enshwick.content.add_item({
    id = CHEST,
    like = enshwick.content.item("Prop_Storage_24_T1_Box"),
    name = enshwick.content.guid(`{CHEST}.name`),
    description = enshwick.content.guid(`{CHEST}.description`),
})
```

The item now exists, but no player can get one. [Items](content/items.md)
lists what else a new item can carry: a meter, tooltip lines, an icon.

## Add its recipe

The recipe is a copy of the storage box's recipe: the same workshop and
unlock, with each ingredient count times `cost`:

```rust
use enshwick::content::recipes::NewRecipe;
use enshwick::content::{Id, id, vanilla};

const CHEST: Id = id!("chest");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.add_recipe(NewRecipe {
        id: CHEST.part("recipe"),
        like: vanilla::item("Prop_Storage_24_T1_Box")?,
        output: CHEST.guid(),
        output_count: 1,
        cost: 2,
        workshop: None,
    })
}
```

```luau
local CHEST = "you.travel-chest:chest"

enshwick.content.add_recipe({
    id = `{CHEST}.recipe`,
    like = enshwick.content.item("Prop_Storage_24_T1_Box"),
    output = enshwick.content.guid(CHEST),
    output_count = 1,
    cost = 2,
})
```

Players now craft the chest where they craft the storage box, for twice its
ingredients. [Recipes](content/recipes.md) moves a recipe to another
workshop and derives many recipes at once.

## Let the admin set the cost

A setting belongs to each world. The loader writes it to a file beside the
world's save, and an admin changes it on the Mods screen. The recipe asks
for its cost each time the game builds the recipe list, and the settings
handler has the game build it again. This is the whole `src/lib.rs` after
its `//!` line:

```rust,standalone_crate
use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::content::items::NewItem;
use enshwick::content::recipes::{NewRecipe, rebuild_recipes};
use enshwick::content::{Id, id, vanilla};
use enshwick::{Load, Mod, Off};

const CHEST: Id = id!("chest");
const CHEST_NAME: Id = CHEST.part("name");
const CHEST_DESCRIPTION: Id = CHEST.part("description");

/// Travel chest settings.
#[enshwick::config]
#[derive(Debug)]
struct Settings {
    /// The chest costs the storage box's ingredients times this.
    #[config(range = 1..=5)]
    cost: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { cost: 2 }
    }
}

/// The cost the recipe asks for; each world's settings set it.
static COST: AtomicU32 = AtomicU32::new(2);

/// The mod.
#[derive(Debug)]
pub struct TravelChest;

impl Mod for TravelChest {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let storage_box = vanilla::item("Prop_Storage_24_T1_Box")?;
        let texts = load.texts()?;
        load.add_texts(&texts, &[
            ("chest.name", CHEST_NAME),
            ("chest.description", CHEST_DESCRIPTION),
        ])?;
        load.add_item(NewItem {
            name: Some(CHEST_NAME.guid()),
            description: Some(CHEST_DESCRIPTION.guid()),
            ..NewItem::new(CHEST, storage_box)
        })?;
        load.add_recipe_with(move || NewRecipe {
            id: CHEST.part("recipe"),
            like: storage_box,
            output: CHEST.guid(),
            output_count: 1,
            cost: COST.load(Ordering::Relaxed),
            workshop: None,
        })?;
        load.on_settings(|settings: &Settings| {
            COST.store(settings.cost, Ordering::Relaxed);
            if let Err(off) = rebuild_recipes() {
                enshwick::warn!("the cost applies at the next start: {off}");
            }
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(TravelChest);
```

The setting's label and description on the Mods screen are texts too. Add
them to `src/texts.toml`, in both languages:

```toml
[en]
"config.cost" = "Cost"
"config.cost.doc" = "The chest costs the storage box's ingredients times this."

[uk]
"config.cost" = "Ціна"
"config.cost.doc" = "Скриня коштує як інгредієнти ящика, помножені на це число."
```

In Luau, the setting goes into `pack.toml`, and the recipe reads it. This
`add_recipe_with` call takes the place of the `add_recipe` call of the
step before:

```toml
[[setting]]
key = "cost"
default = 2
min = 1
max = 5
doc = "The chest costs the storage box's ingredients times this."
```

```luau
local CHEST = "you.travel-chest:chest"

enshwick.content.add_recipe_with(function()
    return {
        id = `{CHEST}.recipe`,
        like = enshwick.content.item("Prop_Storage_24_T1_Box"),
        output = enshwick.content.guid(CHEST),
        output_count = 1,
        cost = enshwick.settings().cost,
    }
end)
enshwick.on_settings(function()
    enshwick.content.rebuild_recipes()
end)
```

[Settings](settings.md) has every kind of setting and when a change
applies.

## Try it

1. In the crate's folder, `travel-chest/`, run
   `cargo enshwick install --game <game folder>`. For the Luau mod, copy its
   folder into the game's `mods/` folder.
2. Start the game and load a world. The mod's log,
   `logs/you.travel-chest_<date>_<time>.log`, shows no error.
3. Open the workshop where the storage box is crafted: the travel chest is
   listed beside it, for twice the box's ingredients.
4. On the Mods screen, open the mod's settings and set the cost to 3. Load
   the world again: the chest costs three times the box's ingredients.

The mod's tests run without the game, on the [fake loader](testing.md).

## What to read next

Each chapter of Everyday tasks covers one subject: [events](events.md),
[logging](logging.md), [the world](world.md), [chat](chat.md). Read
[Concepts](concepts.md) once to see how the loader starts mods and keeps
players in step.

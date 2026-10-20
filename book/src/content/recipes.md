# Recipes

Two calls add recipes:

- [`Load::add_recipe`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/recipes.rs)
  adds one recipe for a new output, copied from the recipe of a vanilla item;
- [`Load::derive_recipes`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/recipes.rs)
  looks at every recipe in the registry and adds a scaled copy of each one
  you pick.

Both patch the game's recipe registry, which every mod shares; patches from
several mods run in turn and their recipes end up side by side.

## A single recipe

```rust
use enshwick_api::content::Guid;
use enshwick_api::content::recipes::NewRecipe;

const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
const SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
const SAFE_RECIPE: Guid = Guid::parse("4e9d2c17-8b3a-4f65-a1c2-7d5e3b9f0a48");

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    load.add_recipe(NewRecipe {
        guid: SAFE_RECIPE,
        like: CHEST,
        output: SAFE,
        output_count: 1,
        cost: 2,
        workshop: None,
    })
}
```

The new recipe is a copy of the first recipe that makes `like`: same
workshop, unlock and duration. It makes `output_count` of `output`, and each
of its ingredient counts is the copied recipe's times `cost`. Here the Safe
is crafted where the wooden chest is, for twice the chest's ingredients.

| Field | Meaning |
| --- | --- |
| `guid` | The recipe's GUID, fixed for good; its game id is `guid.id()`. |
| `like` | The item whose recipe is copied. |
| `output` | The item the recipe makes, such as one from [`add_item`](items.md). |
| `output_count` | How many it makes. |
| `cost` | The copied ingredient counts times this. |
| `workshop` | Where it is made: `None` keeps the copied recipe's workshop, `Some(id)` moves it, such as to a [workshop of the mod's own](workshops.md). |

A `like` that no recipe makes is found out only when the game first reads
the registry: the patch fails and the mod is turned off then, not at `init`.

## Derived recipes

`derive_recipes(salt, choose)` calls `choose` with a view of each recipe in
the registry. Return `Some(Derived { .. })` to add a copy of that recipe
beside it, `None` to leave it alone:

```rust
use enshwick_api::content::recipes::{Derived, RecipeView};

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    load.derive_recipes("building-yield", |recipe: &RecipeView<'_>| {
        recipe
            .debug_name()
            .starts_with("Block_")
            .then_some(Derived { output: 2, cost: 1, workshop: None })
    })
}
```

Each copy has the original's workshop and unlock, unless `workshop` moves
it; its output count is the original's times `output`, and its ingredient
counts are the original's times `cost`. The original stays as it is. The
copy makes the original's first output only.

[`RecipeView`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/recipes.rs)
shows what a recipe is:

- `guid()`, the recipe's GUID;
- `debug_name()`, such as `Block_T2_Metal_Copper_CRAFTING`;
- `output()`, the item it makes and how many, `None` for a recipe without
  output.

The view lists the registry as it reaches this patch, so it includes the
recipes of mods that patched before yours, and the copies of your own
earlier `derive_recipes` calls. A copy keeps the debug name of the recipe it
copies; pick by name with that in mind.

### The salt

A copy needs a GUID of its own. The loader derives it from the original's
GUID, marked with a hash of `salt`, so it stays the same on every start and
differs between mods that copy the same recipe. Use the mod id as the salt,
and never change it: saves hold the copies' ids like any other.

## The workshop limit

The game takes at most 512 recipes at one workshop
([`WORKSHOP_RECIPES`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/recipes.rs));
more ends the game. When any mod serves the recipe registry, the loader
patches the game to lift that limit, on the server and the client alike. If
it cannot, because the game code does not read as the loader expects, the
loader logs `workshop recipe limit kept: <reason>` and the limit holds.

[`workshop_limit()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/recipes.rs)
tells which: `usize::MAX` once the limit is lifted, else 512. The answer is
final before the game first reads the registry, which is when the recipe
patches run, so both calls follow it:

- while the limit holds, derived copies stop
  [`RESERVED_FOR_SINGLE`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/content/recipes.rs)
  (16) short of 512 at each workshop, which leaves room for single recipes
  from `add_recipe`, and a single recipe is skipped at a workshop that
  already holds 512;
- once it is lifted, neither stops.

Copies count against the workshop they are made at, so a copy moved to a new
workshop does not take room at the vanilla one.

### What was left out

Nothing is left out silently. In the mod's own log:

- `recipe skipped: workshop <id> holds 512 recipes, the game's most` for a
  single recipe;
- `<n> derived recipes skipped: a workshop takes 512 recipes at most, 16 kept
  for single ones; ...` for derived copies.

With the limit lifted, the loader's log names each workshop past the game's
number: `workshop <id>: <n> recipes, past the game's 512`.

The vanilla workshops are shared by every mod. A mod that adds many recipes
should give them a [workshop of its own](workshops.md) or an NPC of its own
rather than crowd the vanilla ones; that also keeps it working where the
limit holds.

## A worked example

[`enshwick-building-yield`](https://github.com/themaiby/enshwick/blob/main/mods/enshwick-building-yield/src/lib.rs)
adds bulk recipes for building blocks, overgrowth and decor at a Bulk
Workbench of its own. It calls `derive_recipes` twice, materials first, so
that where a workshop runs out of room, decor gives way. Each call picks
categories by debug name and returns the same `Derived` with the mod's
workshop.

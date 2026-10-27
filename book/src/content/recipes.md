# Recipes

Two calls add recipes:

- [`Load::add_recipe`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
  adds one recipe for a new output, copied from the recipe of a vanilla item;
- [`Load::derive_recipes`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
  looks at every recipe in the registry and adds a scaled copy of each one
  you pick.

Both patch the game's recipe registry, which every mod shares; patches
from several mods run in turn, so their recipes end up side by side. To
change the counts, cost or time of vanilla recipes, declare a data change
with
[`Load::change_recipes`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/changes/recipes.rs)
([the fields](packs.md)) or set a recipe output [rate](../rates.md). To take
recipes out of the game, see [Disabling recipes](disabled-recipes.md).

## A single recipe

```rust
use enshwick::content::{Guid, Id};
use enshwick::content::recipes::NewRecipe;

const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
const LOCKER: Id = Id::new("you.lockers:locker");
const LOCKER_RECIPE: Id = Id::new("you.lockers:locker.recipe");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.add_recipe(NewRecipe {
        id: LOCKER_RECIPE,
        like: CHEST,
        output: LOCKER.guid(),
        output_count: 1,
        cost: 2,
        workshop: None,
    })
}
```

The new recipe is a copy of the first recipe that makes `like`: same
workshop, unlock and duration. It makes `output_count` of `output`, and each
of its ingredient counts is the copied recipe's times `cost`. Here the locker
is crafted where the wooden chest is, for twice the chest's ingredients.

| Field | Meaning |
| --- | --- |
| `id` | The recipe's [content id](../content.md#content-ids), fixed for good; its game id is `RecipeId::from_guid(id.guid())`. |
| `like` | The item whose recipe is copied. |
| `output` | The item the recipe makes, such as one from [`add_item`](items.md), by GUID: `Guid::named` of a mod item's id, or a vanilla GUID. |
| `output_count` | How many it makes. |
| `cost` | The copied ingredient counts times this. |
| `workshop` | Where it is made: `None` keeps the copied recipe's workshop, `Some(id)` moves it, such as to a [workshop of the mod's own](workshops.md). |

A `like` that no recipe makes is found out only when the game first reads
the registry: the patch fails and the mod is turned off then, not at `init`.

When the cost comes from a setting that changes, such as a world's or a
server's join value, `load.add_recipe_with(|| NewRecipe { .. })` asks the
closure each time the registry is built, and
`content::recipes::rebuild_recipes()` from the settings handler builds it
again with the new cost ([Rebuilding](resources.md#rebuilding)).

## Derived recipes

`derive_recipes(salt, choose)` calls `choose` with a view of each recipe in
the registry. Return `Some(Derived { .. })` to add a copy of that recipe
beside it, `None` to leave it alone:

```rust
use enshwick::content::recipes::{Derived, RecipeView};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.derive_recipes("my-mod", |recipe: &RecipeView<'_>| {
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

[`RecipeView`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
shows what a recipe is:

- `guid()`, the recipe's GUID;
- `debug_name()`, such as `Block_T2_Metal_Copper_CRAFTING`;
- `output()`, the item it makes and how many, `None` for a recipe without
  output;
- `kind()`, what that item is, from the game's own data
  ([Recipe kinds](#recipe-kinds)).

`choose` sees only the game's own recipes: recipes other mods add, and the
copies of your own earlier calls, are never offered, since the crafting menu
places each copy after a recipe it knows before any mod's registry patch
runs.

### The salt

A copy needs a GUID of its own. The loader derives it from the original's
GUID, marked with a hash of `salt`, so it stays the same on every start and
differs between mods that copy the same recipe. Use the mod id as the salt,
and never change it: saves hold the copies' ids like any other.

## Recipe kinds

`RecipeView::kind()` says what the item a recipe makes is, as the game's
own item data classes it:

| `RecipeKind` | Data pack name | Items |
| --- | --- | --- |
| `BuildingBlock` | `building-block` | equipment slot `BlueprintMaterial_Default`: building blocks |
| `Roof` | `roof` | equipment slot `BlueprintMaterial_Roof`: roof blocks |
| `Overgrowth` | `overgrowth` | equipment slot `OvergrowthMaterial` |
| `Terrain` | `terrain` | equipment slot `TerrainMaterial` |
| `Decorative` | `decorative` | placed props that carry the item tag of the crafting menu's Decorative group: decorations and much of the furniture |
| `Other` | `other` | everything else: equipment, functional props, materials |

Match it with a `_` arm: later versions may add kinds. Prefer it to a debug
name prefix, which misses recipes whose names break the pattern. The game
data files a prop under a menu group by its item tag, so `Decorative`
follows the game's own grouping rather than a name.

The same kinds pick recipes for a data change, `Recipes::Kind`, and for a
recipe output rate, `Rate::RecipeOutput`:

```rust
use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
use enshwick::content::recipes::RecipeKind;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let decor = RecipesChange::new(
        Recipes::Kind(RecipeKind::Decorative),
        Field::Cost,
        Value::Times(0.5),
    );
    load.change_recipes(&decor, "cheaper decorations")
}
```

The loader classes the vanilla recipes for data changes from the game's
archive, so they apply from the first build on every side. `kind()` learns
an item's kind when the game loads the item and answers `None` before that.
A dedicated server builds its recipe registry before it loads the items, so
`derive_recipes` sees no kinds there on its first build.

## The crafting menu

The client's crafting menu does not list what the registry holds. It lists
the recipe ids of a tree baked into the game's UI bundle: per workshop,
groups of sets of recipes. So `add_recipe`, `add_recipe_with` and
`derive_recipes` also patch the UI bundle on the client, and the
[`content::menu`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/menu.rs)
module does the edit:

- a new recipe goes right after the vanilla recipe that makes `like`, in the
  same set;
- a derived copy goes right after its original; `derive_recipes` copies
  only the game's own recipes, never those mods add;
- a recipe whose `workshop` names another workshop goes last in that
  workshop's first set, unless the copied recipe is listed there.

The menu is shared like the registry: each mod's patch runs on the bundle as
the mods before it left it. Each new recipe goes right after the one it
copies, so of the locker of one mod and the Vault of a mod loaded later, both
copied from the wooden chest, the Vault shows first: chest, Vault, locker. One group of
the menu shows at most 512 recipes, all its sets together
([`GROUP_ENTRIES`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/menu.rs));
the mod's log says when a recipe is left out because its group is full or
because nothing in the menu says where it goes. The dedicated server has no
crafting menu and serves no UI bundle.

## The workshop limit

The game takes at most 512 recipes at one workshop
([`WORKSHOP_RECIPES`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs));
more ends the game. When any mod serves the recipe registry, the loader
patches the game to lift that limit, on the server and the client alike. If
it cannot, because the game code does not read as the loader expects, the
loader logs `workshop recipe limit kept: <reason>` and the limit holds.

[`workshop_limit()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
tells which: `usize::MAX` once the limit is lifted, else 512. The answer is
final before the game first reads the registry, which is when the recipe
patches run, so both calls follow it:

- while the limit holds, derived copies stop
  [`RESERVED_FOR_SINGLE`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
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

<!--
type: how-to
description: A server owner takes vanilla or modded recipes out of a world with one file beside its save and no mod. A mod disables recipes too, through recipes::disable.
-->

# Disabling recipes

A server owner takes recipes out of a world, vanilla ones or a mod's, with
a file and no mod: `<stem>-mod-settings/enshwick/recipes.toml` beside the
world's save. Mods disable recipes too, through
[`recipes::disable`](#from-a-mod).

## What disabling does

A disabled recipe leaves every crafting menu, by hand, at stations and at
survivors, and the server refuses to craft it. Items that exist stay as they
are, and administrators still hand them out with a mod's `give` action,
which makes items without a recipe.

## The recipe file

The loader writes the file as the world first loads, with every list
empty; filled, it reads:

```toml
[disable]
recipes = ["enshrouded:Tool_Torch01_T0_PK", "you.lockers:locker.recipe"]
outputs = ["enshrouded:Tool_Torch01_T0"]
workshops = ["hand", "enshrouded:NPC_Crafter_Blacksmith01"]

[enable]
recipes = ["enshrouded:Armor_T1_Grassland_Fighter_Arms"]
outputs = []
workshops = []
```

Every name is a full id: the game's recipes, items and workshops as
`enshrouded:<debug name>`, a mod's by its [content id](../content.md#content-ids),
`<mod id>:<name>`, and any by its GUID. A debug name without `enshrouded:`
is a problem in the log that says how to write it.

| List | Disables |
| --- | --- |
| `recipes` | one recipe |
| `outputs` | every recipe that makes an item |
| `workshops` | every recipe made at a workshop: `hand` for crafting by hand, or the id of the station's or survivor's item, such as `enshrouded:Craftingstation_Workbench` or `enshrouded:NPC_Crafter_Blacksmith01` |

`[enable]` takes recipes back, in the same forms, from the file's `[disable]`
and from mods' rules: the example disables every recipe of the Blacksmith
but one.

A name the loader cannot resolve is a line in the loader's log with the
file, the line and the names that look like it, and the other rules still
apply:

```text
warn  [enshwick] disabled recipes: savegame/3ad85aea-mod-settings/enshwick/recipes.toml:3: no recipe "enshrouded:Tool_Torch01_T0_PKK"; near: enshrouded:Tool_Torch01_T0_PK
info  [enshwick] disabled recipes: 224
```

A debug name several recipes carry is a problem too, with their GUIDs to
pick from.

## Worlds

Each world has its own file, and no file applies to every world: the
dedicated server's world, and each world of a player who plays alone or
hosts, keeps its rules beside its save, so they travel with it. The rules
apply in turn: mods' rules, then the world's file, which disables what its
`[disable]` names and enables back what its `[enable]` names. A
`mod-configs/` folder of an older loader is not read; the log says so once
at start.

## Changes while the game runs

The loader looks at the running world's file once a second and applies a change at once,
on the dedicated server and in a game a player hosts: menus lose or regain
the recipes and the server's refusals follow. No restart is needed.

## Finding names and changing rules from the console

The server's [console](../chat.md#the-console) finds names and changes the
rules:

| Line | Answer |
| --- | --- |
| `enshwick recipes` | the disabled recipes, each with its GUID, workshop and output |
| `enshwick recipes find <text>` | recipes whose debug name or whose output's name holds the text, the disabled ones marked |
| `enshwick recipes disable <recipe\|output\|workshop> <name>` | checks the name and adds it to the running world's file; while no world runs, it answers that a world must run |
| `enshwick recipes enable <recipe\|output\|workshop> <name>` | the same for `[enable]` |

```text
enshwick recipes find torch01
Recipes that match "torch01": 1.
enshrouded:Tool_Torch01_T0_PK (7fa62007-a38e-4790-8591-873055fc3f34) at hand, makes enshrouded:Tool_Torch01 (item 1234) (disabled)
```

## Players who join

The server decides. A client that runs Enshwick and joins gets the server's
disabled recipes over the loader's messages and hides them from its menus;
its own worlds' recipe files do not count while it plays there, and apply again
when it hosts or plays alone. A client without Enshwick keeps showing the
recipes, and the server still refuses to craft them, so nothing is made and
nothing is taken.

## How it works

The game keeps the recipes of each workshop in a list its menus and its
crafting ask for by workshop id; crafting by hand is workshop 0. The loader
hooks that lookup and hands out each list without the disabled recipes. On
the server it also screens each craft request before the crafting system
reads it, as [vetoes](../vetoes.md#crafting) do, and empties a request for a
disabled recipe.

## From a mod

[`recipes::disable`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
adds a rule of the mod's, in the forms of the file, at init or later, from
any thread; `recipes::enable` takes the mod's own rule back, and
`recipes::is_disabled` asks whether a recipe, by GUID or content id, is
disabled now. A rule names its recipe, item or workshop item by `Guid`,
content `Id` or `DebugName`, which goes to the loader as
`enshrouded:<debug name>`, and `RecipeRule::Hand` stands for crafting by
hand:

```rust
use enshwick::content::recipes::{RecipeRule, disable, is_disabled};
use enshwick::content::{DebugName, Id, id};

const LOCKER_RECIPE: Id = id!("locker.recipe");

fn init(_load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // Torches come from the mod's own loot, not from crafting.
    disable(RecipeRule::output(DebugName("Tool_Torch01_T0")))?;
    disable(RecipeRule::workshop(DebugName("Craftingstation_Workbench")))?;
    if !is_disabled(LOCKER_RECIPE) {
        enshwick::info!("lockers are craftable");
    }
    Ok(())
}
```

```luau
-- Torches come from the mod's own loot, not from crafting.
enshwick.content.disable({ Output = "Tool_Torch01_T0" })
enshwick.content.disable({ Workshop = "Craftingstation_Workbench" })
if not enshwick.content.is_disabled(enshwick.content.guid("you.lockers:locker.recipe")) then
    enshwick.log.info("lockers are craftable")
end
```

A Luau rule is a table, `{ Recipe = name }`, `{ Output = name }` or
`{ Workshop = name }`, or the text `"Hand"`. A name is a GUID, a content id or
a debug name, and `is_disabled` takes the recipe's GUID.

The file wins: mods' rules apply first, so a world's `[enable]` takes back
any recipe a mod disables, and a mod's `enable` takes back only its own
rules. A name the loader cannot resolve is logged in the mod's log and the
loader's. On a client joined to a server, the server's rules apply, mods'
rules on the server included, and the client's mods' rules wait until it
hosts or plays alone.

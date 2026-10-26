# Disabling recipes

A server owner takes recipes out of the game, vanilla ones or a mod's, with
a file and no mod: `mod-configs/enshwick.recipes.toml`. A disabled recipe
leaves every crafting menu, by hand, at stations and at survivors, and the
server refuses to craft it. Items that exist stay as they are, and
administrators still hand them out with a mod's `give` action, which
makes items without a recipe. Mods disable recipes too, through
[`recipes::disable`](#from-a-mod).

## The recipe file

The loader writes the file on first start, with every list empty:

```toml
[disable]
recipes = ["Tool_Torch01_T0_PK", "you.lockers:locker.recipe"]
outputs = ["Tool_Torch01_T0"]
workshops = ["hand", "NPC_Crafter_Blacksmith01"]

[enable]
recipes = ["Armor_T1_Grassland_Fighter_Arms"]
outputs = []
workshops = []
```

| List | Disables |
| --- | --- |
| `recipes` | one recipe: its debug name, a mod's recipe by its [content id](../content.md#content-ids), any by its GUID |
| `outputs` | every recipe that makes an item: the item's debug name, content id or GUID |
| `workshops` | every recipe made at a workshop: `hand` for crafting by hand, the debug name of the station's or survivor's item, such as `Craftingstation_Workbench` or `NPC_Crafter_Blacksmith01`, or the workshop's content id or GUID |

`[enable]` takes recipes back, in the same three forms, from the file's
`[disable]` and from mods' rules: the example disables every recipe of the
Blacksmith but one. Debug names of items and workshops come from the game's
data, so name a mod's item or workshop by its content id.

A name the loader cannot resolve is a line in the loader's log with the
file, the line and the names that look like it, and the other rules still
apply:

```text
warn  [enshwick] disabled recipes: mod-configs/enshwick.recipes.toml:3: no recipe "Tool_Torch01_T0_PKK"; near: Tool_Torch01_T0_PK
info  [enshwick] disabled recipes: 224
```

A debug name several recipes carry is a problem too, with their GUIDs to
pick from.

## Worlds

The file in `mod-configs` holds the rules of every world the process runs:
the dedicated server's world, or each world of a player who plays alone or
hosts. A world's own file, `mod-data/enshwick/worlds/<key>/recipes.toml`, in
the same form, applies over it while that world runs, so a host can disable
recipes in one world only. The rules apply in turn: mods' rules, then the
`mod-configs` file, then the world's; each file disables what its
`[disable]` names and enables back what its `[enable]` names, over
everything before it.

## Changes while the game runs

The loader looks at both files once a second and applies a change at once,
on the dedicated server and in a game a player hosts: menus lose or regain
the recipes and the server's refusals follow. No restart is needed.

## Finding names and changing rules from the console

The server's [console](../chat.md#the-console), such as a mod's command
file, finds names and changes the rules:

| Line | Answer |
| --- | --- |
| `enshwick recipes` | the disabled recipes, each with its GUID, workshop and output |
| `enshwick recipes find <text>` | recipes whose debug name or whose output's name holds the text, the disabled ones marked |
| `enshwick recipes disable <recipe\|output\|workshop> <name>` | checks the name and adds it to the running world's file (the `mod-configs` file while no world runs) |
| `enshwick recipes enable <recipe\|output\|workshop> <name>` | the same for `[enable]` |

```text
enshwick recipes find torch01
Recipes that match "torch01": 1.
Tool_Torch01_T0_PK (7fa62007-a38e-4790-8591-873055fc3f34) at hand, makes Tool_Torch01 (disabled)
```

## Players who join

The server decides. A client that runs Enshwick and joins gets the server's
disabled recipes over the loader's messages and hides them from its menus;
its own recipe files do not count while it plays there, and apply again
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
content `Id` or `DebugName`, and `RecipeRule::Hand` stands for crafting by
hand:

```rust
use enshwick::content::recipes::{RecipeRule, disable, is_disabled};
use enshwick::content::{DebugName, Id};

const LOCKER_RECIPE: Id = Id::new("you.lockers:locker.recipe");

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

The file wins: mods' rules apply first, so a server's `[enable]` takes back
any recipe a mod disables, and a mod's `enable` takes back only its own
rules. A name the loader cannot resolve is logged in the mod's log and the
loader's. On a client joined to a server, the server's rules apply, mods'
rules on the server included, and the client's mods' rules wait until it
hosts or plays alone.

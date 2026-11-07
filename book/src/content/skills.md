<!--
type: how-to
description: Mods add skill nodes to the skill tree within the room a player's save keeps, linked from vanilla nodes, and perks copied from vanilla ones.
-->

# Skill nodes and perks

A mod adds skill tree nodes and perks. A node copies a vanilla node's effect
and icons, at its own place, linked from vanilla or mod nodes. A perk copies a
vanilla perk with its own texts.

## A node of the mod's own

`load.skill_nodes()` lists the game's nodes, each with its id, kind and place.
The kind is one of these:

- `Root`
- `Attribute`
- `ActiveSkill`
- `PassiveSkill`

On server 399 the tree has 243 nodes, 12 of them roots.

`load.add_skill_node(NewSkillNode)` adds a copy of the vanilla node `like`:

| Field | What it sets |
| --- | --- |
| `id` | the node's content id |
| `like` | the vanilla node whose effect and icons it copies |
| `x`, `y` | where the tree screen draws it |
| `links_from` | the nodes it is unlocked from, vanilla or a mod's |
| `name`, `description` | its texts' tags |
| `cost` | the skill points it costs; nil keeps the vanilla node's |
| `max_level` | 0 for a copy of a node with levels; nil keeps the vanilla node's |
| `kind` | what it is; nil keeps the vanilla node's |

A player unlocks it with skill points as the game's own nodes. The tree
screen draws it from the data on the client, so a mod that adds one runs on
both sides.

```rust
use enshwick::content::id;
use enshwick::content::skills::NewSkillNode;
use enshwick::typed::SkillNodeType;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let tree = load.skill_nodes()?;
    let root = tree.iter().find(|node| node.kind == SkillNodeType::Root);
    let stamina = tree.iter().find(|node| node.kind == SkillNodeType::Attribute);
    if let (Some(root), Some(stamina)) = (root, stamina) {
        load.add_skill_node(NewSkillNode {
            links_from: vec![root.id],
            cost: Some(1),
            ..NewSkillNode::new(id!("endurance"), stamina.id, root.x + 120.0, root.y, id!("endurance.name").guid(), id!("endurance.text").guid())
        })?;
    }
    Ok(())
}
```

## How many nodes fit

A player's save keeps a fixed count of unlocked nodes: 256 on server 399, as
the running server's reflection reports. The game's tree takes 243 of them,
so 13 nodes fit for every mod together, in load order. A node beyond the room
returns `Err(Off)` when the game's own tree is full already. When other mods'
nodes fill the room, the loader leaves the node out as the game first reads the
tree. On a
server copy of server 399, the 14th node of a mod was left out this way.

Each node shows as the mod's feature `skill node <id>` in the status summary
and on the Mods screen. A node that is refused or left out shows the reason.

A node closer to another than half the vanilla tree's smallest gap between
two nodes returns `Err(Off)`. On server 399 that gap is about 103, so a node
keeps about 51 away from every other, mods' nodes included. A place that is
not a finite number, a node linked twice from one node, and a node with
levels also return `Err(Off)`. The game keeps a node's levels in a slot of
its own (`levelIndex`). A copy would share that slot with its vanilla node, so
a copy of a node with levels sets `max_level` to 0.

## A perk of the mod's own

`load.add_perk(NewPerk)` copies the vanilla perk `like` under the mod's id,
with its own name and description, and lists it beside the game's perks.

These calls need the `typed` feature.

## From Luau

The same calls run at the top level of `main.luau`:

- `enshwick.content.skill_nodes()`
- `enshwick.content.add_skill_node{...}`
- `enshwick.content.add_perk{...}`

```luau
local nodes = enshwick.content.skill_nodes()
local root, stamina = nodes[1], nodes[2]
enshwick.content.add_skill_node({
    id = "you.skills:endurance",
    like = stamina.id,
    x = root.x + 120,
    y = root.y,
    links_from = { root.id },
    name = enshwick.content.guid("you.skills:endurance.name"),
    description = enshwick.content.guid("you.skills:endurance.text"),
    cost = 1,
})
```

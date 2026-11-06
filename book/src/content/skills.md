<!--
type: how-to
description: Mods add skill nodes to the skill tree within the room a player's save keeps, and perks copied from vanilla ones.
-->

# Skill nodes and perks

A mod adds nodes to the skill tree: each copies a vanilla node's effect and
icons, at a place of its own, linked from vanilla or mod nodes. A mod also
adds perks copied from vanilla perks with texts of their own.

## A node of the mod's own

`load.skill_nodes()` lists the game's nodes: each one's id, kind (`Root`,
`Attribute`, `ActiveSkill` or `PassiveSkill`) and place. On server 399 the
tree has 243 nodes, 12 of them roots.

`load.add_skill_node(NewSkillNode)` adds a copy of the vanilla node `like`:

| Field | What it sets |
| --- | --- |
| `id` | the node's content id |
| `like` | the vanilla node whose effect and icons it copies |
| `x`, `y` | where the tree screen draws it |
| `links_from` | the nodes it is unlocked from, vanilla or a mod's |
| `name`, `description` | its texts' tags |
| `cost` | the skill points it costs; nil keeps the vanilla node's |
| `max_level` | the levels it has; nil keeps the vanilla node's |
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
returns `Err(Off)` when the game's own tree is full already; past other mods'
nodes, the loader logs it under the mod's name when the game first reads the
tree and leaves it out. On a server copy of server 399, the 14th node of a
mod was left out this way.

A node closer to another than half the tree's smallest gap between two nodes
returns `Err(Off)`: on server 399 that gap is about 103, so a node keeps about
51 away from every other.

## A perk of the mod's own

`load.add_perk(NewPerk)` copies the vanilla perk `like` under the mod's id,
with its own name and description, and lists it beside the game's perks.

These calls need the `typed` feature.

## From Luau

The same calls are `enshwick.content.skill_nodes()`,
`enshwick.content.add_skill_node{...}` and `enshwick.content.add_perk{...}`,
at the top level of `main.luau`.

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

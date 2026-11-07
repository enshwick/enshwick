<!--
type: how-to
description: Mods add knowledge entries of the world or of each player, with skill points and experience, and test entries in quests, pages and skill nodes.
-->

# Knowledge entries

A mod adds knowledge entries beside the game's: a bool, a counter or flags,
per world or per player. The game grants, sends and saves them like
its own, so quests, pages and skill nodes rest on them.

## An entry of the mod's own

`load.add_knowledge(NewKnowledge)` adds the entry and returns its knowledge
id, the [`KnowledgeId::from_guid`] of the content id:

| Field | What it sets |
| --- | --- |
| `id` | the entry's content id |
| `scope` | `World`, shared by every player, or `Player`, each player's own |
| `kind` | `Bool`: a bool. `Counter`: a counter. `Flags`: flags |
| `flags` | the bits a flags entry may hold; 0 for the others |
| `skill_points` | the skill points a player's entry gives when granted |
| `experience` | the experience value a player's entry gives when granted |

A world's entry gives no skill points or experience: the call returns
`Err(Off)`. An entry is read on the server and the client, so a mod that adds
one runs on both sides. [Knowledge](../knowledge.md) reads and grants it.

```rust
use enshwick::content::id;
use enshwick::content::knowledge::NewKnowledge;
use enshwick::knowledge::{Grant, KnowledgeType, Scope};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let shrine = load.add_knowledge(NewKnowledge {
        skill_points: 1,
        ..NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool)
    })?;
    let knowledge = load.knowledge();
    load.on::<enshwick::events::PlayerJoined>(move |joined| {
        let _ = knowledge.grant(joined.entity, shrine, Grant::Set);
    })
}
```

These calls need the `typed` feature.

## Testing an entry

A `KnowledgeQuery` tests one entry, vanilla or a mod's, where quests, journal
pages and skill nodes ask for knowledge. It is one of:

- `KnowledgeQuery::is_set(id, scope)`, which holds when a bool is set;
- `KnowledgeQuery::at_least(id, scope, count)`, which holds when a counter
  reaches `count`.

A player's entry is tested for the player the game asks about.

## From Luau

The same call is `enshwick.content.add_knowledge{...}`, at the top level of
`main.luau`; a query is a table.

```luau
local shrine = enshwick.content.add_knowledge({
    id = "you.lore:found-the-shrine",
    scope = "Player",
    kind = "Bool",
    skill_points = 1,
})
local found = { id = shrine, scope = "Player", test = "IsSet" }
```

## When the mod is removed

A save keeps the values of the mod's entries while the mod runs. A save made
without the mod drops them, so they read 0 when it comes back. On a server
copy of server 399, the world opened and saved without errors.

[`KnowledgeId::from_guid`]: https://docs.rs/enshwick/latest/enshwick/knowledge/struct.KnowledgeId.html#method.from_guid

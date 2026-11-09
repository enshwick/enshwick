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
id, the [`KnowledgeId::from_guid`] of the [content id](../concepts/ids.md):

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

## Give an amount of experience

An entry's `experience` and a quest's `QuestReward::Experience` name an
experience value of the game's balancing table: a source and an amount.

`vanilla::experience_values()` lists the game's own values. Each has a value,
a source and a gain. `load.add_experience(NewExperience)` adds a value of the
mod's own and returns it:

| Field | What it sets |
| --- | --- |
| `id` | the value's content id; the value is the FNV-1a of its GUID |
| `source` | the source the game counts it under, an `ExperienceSourceType` |
| `gain` | the experience it gives |

The world's experience factor of the source's `ExperienceGroup` scales the
gain, as for a vanilla value. A source in no group, such as `None`, has no
factor. Adding the same id again replaces its source and gain. A source the
game build lacks returns `Err(Off)`. So does an `Other` number the build's
source enum does not hold. These calls need the `typed` feature.

```rust
use enshwick::content::id;
use enshwick::content::knowledge::{NewExperience, NewKnowledge};
use enshwick::game_events::ExperienceSourceType;
use enshwick::knowledge::{KnowledgeType, Scope};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let found = load.add_experience(NewExperience::new(id!("shrine-xp"), ExperienceSourceType::Exploration, 250))?;
    load.add_knowledge(NewKnowledge {
        experience: Some(found),
        ..NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool)
    })?;
    Ok(())
}
```

On a server copy of server 224, the game built its balancing table with an
added value at start, 182 values in place of 181. Whether a player gets the
gain of an added value is not yet seen in the game, so the
[stability](../stability.md) chapter lists `Load::add_experience` as
unstable.

## Testing an entry

A `KnowledgeQuery` tests one entry, vanilla or a mod's, or one of the
game's own queries, where quests, journal pages and skill nodes ask for
knowledge. Its `test` is one of:

| Test | Holds when |
| --- | --- |
| `IsSet` (`KnowledgeQuery::is_set`) | the value is not 0: a bool set, a counter or flags not 0 |
| `AtLeast(n)` (`KnowledgeQuery::at_least`) | a counter is at `n` or above, from 1 |
| `Equals(n)` | a counter is at exactly `n`; `Equals(0)` is an entry not set |
| `Below(n)` | a counter is below `n`, from 1 |
| `HasFlag(bits)` | flags hold any of `bits` |
| `Holds` (`KnowledgeQuery::game`) | the game's query of the id holds |
| `Fails` | the game's query of the id does not hold |

`query.not()` gives the opposite test, where the game has one: none for an
exact count other than 0 or for flags. A player's entry is tested for the
player the game asks about.

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

Experience values are `enshwick.content.experience_values()` and
`enshwick.content.add_experience{...}`:

```luau
local xp = enshwick.content.add_experience({ id = "you.lore:shrine-xp", source = "Exploration", gain = 250 })
enshwick.content.add_knowledge({ id = "you.lore:found-it", scope = "Player", kind = "Bool", experience = xp })
```

## When the mod is removed

A save keeps the values of the mod's entries while the mod runs. A save made
without the mod drops them, so they read 0 when it comes back. On a server
copy of server 399, the world opened and saved without errors.

[`KnowledgeId::from_guid`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi/ids.rs

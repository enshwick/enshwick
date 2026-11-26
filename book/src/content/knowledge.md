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
    let mut knowledge_2 = NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool);
    knowledge_2.skill_points = 1;
    let shrine = load.add_knowledge(knowledge_2)?;
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
    let mut knowledge_2 = NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool);
    knowledge_2.experience = Some(found);
    load.add_knowledge(knowledge_2)?;
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
game's own queries. Quests, journal pages and skill nodes ask for knowledge
with it. Its `test` is one of:

| Test | Holds when |
| --- | --- |
| `IsSet` (`KnowledgeQuery::is_set`) | the value is not 0: a bool set, a counter or flags not 0 |
| `AtLeast(n)` (`KnowledgeQuery::at_least`) | a counter is at `n` or above, from 1 |
| `Equals(n)` | a counter is at exactly `n`; `Equals(0)` is an entry not set |
| `Below(n)` | a counter is below `n`, from 1 |
| `HasFlag(bits)` | flags hold any of `bits` |
| `Holds` (`KnowledgeQuery::game`) | the game's query of the id holds |
| `Unknown` | only the game answers: a form of the game's that enshwick does not read |

`query.not()` gives the opposite test, where the game has one. It has none
for an exact count other than 0, for flags, for a game query or for
`Unknown`. The game reads a game query's answer alone, with no compare.
So the opposite of one is a query of the mod's own that inverts it. A mod cannot write
`Unknown`; it only reads back from the game's own steps. A player's entry is
tested for the player the game asks about.

## A query of the mod's own

`Load::add_query` adds a query to the game's query database, beside the
game's own 5280 named queries (server 26430253). The game evaluates it as
it evaluates its own. A quest step, a lore entry or an item set asks it
with `KnowledgeQuery::game` and reads the game's answer. The call returns
its `GameQuery`.

| Field | What it is |
| --- | --- |
| `id` | its content id; its query id is the FNV-1a of the id's GUID |
| `scope` | `Player` for a query asked for each player, `World` for the world |
| `form` | how the parts join: all, any, or exactly one of two (`QueryForm`) |
| `invert` | whether it holds where its form does not |
| `parts` | knowledge tests, or game queries: the game's, or ones a mod added before it |

`invert` is the one way to the opposite of a game query. The game reads
a game query's answer and no compare, so `KnowledgeQuery::not` has none.

`Load::game_query(name)` finds one of the game's queries by the name its
database gives it, such as `ItemSetCompleted_Grassland`.

```rust
use enshwick::content::id;
use enshwick::content::knowledge::{KnowledgeQuery, NewQuery, QueryForm};
use enshwick::knowledge::Scope;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let grassland = load.game_query("ItemSetCompleted_Grassland")?;
    let mut query = NewQuery::new(id!("grassland-unfinished"), Scope::Player, QueryForm::All, vec![KnowledgeQuery::game(grassland)]);
    query.invert = true;
    let unfinished = load.add_query(query)?;
    let _shown_when = KnowledgeQuery::game(unfinished);
    Ok(())
}
```

The database lists the world's queries first and the players' after them.
A world query goes at the end of the world's range, and a player query at
the end. Every index the game keeps past the world's range moves with it.
That includes the index a part keeps of the query it names. One patch of the
database serves all of a mod's queries. When the game first reads the
database, a query whose id another mod's holds is logged and left out. So
is a query whose part names a query the database lacks. This is read in
the code of server 26430253.

## A knowledge rule

`Load::add_knowledge_rule` adds a rule to the game's own trigger list.
While the rule's query holds, the game's server grants its knowledge, and
no mod code runs at that moment. The game unlocks much of its own knowledge
this way.

```rust
use enshwick::content::id;
use enshwick::content::knowledge::KnowledgeQuery;
use enshwick::knowledge::{Grant, KnowledgeId, NewRule, RuleGrant, Scope};

fn init(load: &mut enshwick::Load<'_>, both: KnowledgeQuery, crowned: KnowledgeId) -> Result<(), enshwick::Off> {
    load.add_knowledge_rule(NewRule::new(id!("crown-on-both"), Scope::Player, both, vec![RuleGrant::new(crowned, Grant::Set)]))
}
```

A player's rule runs for each player and a world's for the world, every 32
steps of the world. A grant of knowledge the holder has already is
skipped. A rule sets bools and flags; it cannot add to a counter, since the
game adds on every run while the query holds. The game grants only an
entry its knowledge holds, the game's or a mod's. The list is the server's
alone, so a mod that only adds rules runs on the server. This is read in
the code of server 26430253.

`Load::knowledge_rules()` lists the game's rules with their `RuleId`: the
side and the place in its list. `Load::change_knowledge_rule` changes a
rule's query or grants. `Load::remove_knowledge_rule` turns a rule off. It
keeps its place with a query the game skips, so the places of the rules
after it stay the same for every mod.

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

Queries are `enshwick.content.add_query{...}` and
`enshwick.content.game_query(name)`:

```luau
local grassland = enshwick.content.game_query("ItemSetCompleted_Grassland")
local unfinished = enshwick.content.add_query({
    id = "you.lore:grassland-unfinished",
    scope = "Player",
    form = "All",
    invert = true,
    parts = { { id = grassland, scope = "World", test = "Holds" } },
})
```

Rules take the same names under `enshwick.content`:

- `add_knowledge_rule{...}`
- `knowledge_rules()`
- `change_knowledge_rule(rule, {...})`
- `remove_knowledge_rule(rule)`

A grant is a table such as `{ id = crowned, grant = "Set" }`.

## When the mod is removed

A save keeps the values of the mod's entries while the mod runs. A save made
without the mod drops them, so they read 0 when it comes back. On a server
copy of server 399, the world opened and saved without errors.

[`KnowledgeId::from_guid`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/abi/src/abi/ids.rs

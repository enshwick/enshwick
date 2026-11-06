<!--
type: how-to
description: Mods add quests and lore pages to the journal on knowledge, change vanilla quests and their rewards, and set a player's quest progression.
-->

# Quests and journal pages

A mod adds quests and lore pages to the journal and changes the game's
quests. A quest's steps show and complete on [knowledge](knowledge.md); its
rewards pay out when the last step completes. The game's own screens draw
them.

## A quest of the mod's own

`load.add_quest(NewQuest)` adds the quest beside the game's and returns its
quest id. The quest id is a player's bool the game sets when it pays the
quest, as for its own quests; [Knowledge](../knowledge.md) reads it.

| Field | What it sets |
| --- | --- |
| `id` | the quest's content id |
| `name` | its title's text tag |
| `source` | who gives it, which groups it in the journal |
| `kind` | `PlayerQuest`, `WorldQuest` or `Auto` |
| `main` | whether the journal lists it as a main quest |
| `for_all_players` | whether one player's completion unlocks it for all |
| `steps` | its steps, in order |
| `rewards` | items by GUID and experience values |

Each `QuestStep` shows while `shown_when` holds and is done once `done_when`
holds; `progress` lists parts the journal counts. A mod moves the quest on by
granting the knowledge its steps ask for. Texts come from the mod's texts, in
every language it gives (see [Texts](texts.md)).

```rust
use enshwick::content::{Guid, id};
use enshwick::content::journal::{NewQuest, QuestReward, QuestStep};
use enshwick::content::knowledge::{KnowledgeQuery, NewKnowledge};
use enshwick::knowledge::{KnowledgeType, Scope};

fn init(load: &mut enshwick::Load<'_>, torch: Guid) -> Result<(), enshwick::Off> {
    let given = load.add_knowledge(NewKnowledge::new(id!("trail-given"), Scope::Player, KnowledgeType::Bool))?;
    let wood = load.add_knowledge(NewKnowledge::new(id!("trail-wood"), Scope::Player, KnowledgeType::Counter))?;
    let step = QuestStep::new(
        id!("trail.step").guid(),
        id!("trail.step.text").guid(),
        KnowledgeQuery::is_set(given, Scope::Player),
        KnowledgeQuery::at_least(wood, Scope::Player, 3),
    );
    load.add_quest(NewQuest {
        rewards: vec![QuestReward::Item(torch)],
        ..NewQuest::new(id!("trail"), id!("trail.name").guid(), vec![step])
    })?;
    Ok(())
}
```

On a server copy of server 399, the server's journal listed a mod's quest
beside the game's 188.

## A lore page

`load.add_journal_page(NewJournalPage)` adds a lore page listed under its
category's text tag. Each `PageEntry` shows once its `shown_when` holds.

## Changing a vanilla quest

`load.change_quest(quest, QuestChange)` changes a vanilla quest in memory:

- `rewards` replaces its rewards, and `add_rewards` adds after them;
- `for_all_players` sets whether one player's completion unlocks it for all;
- `steps` replaces the knowledge a step shows or completes on.

Mods that change one quest change it in load order, each on the one before's
result. `knowledge.quests()` lists the quest ids of the running game.

## Personal quest progression

`knowledge.quest_progression(player)` says whether a player follows the
quests on their own rather than with the world, and
`knowledge.set_quest_progression(player, personal)` sets it, as the journal's
own setting does. The server sets it and the game sends it to the clients.

These calls need the `typed` feature.

## From Luau

The same calls are `enshwick.content.add_quest{...}`,
`enshwick.content.add_journal_page{...}` and
`enshwick.content.change_quest(id, {...})` at the top level of `main.luau`,
and `enshwick.knowledge.quest_progression(player)` and
`set_quest_progression(player, personal)` while the world runs.

```luau
local given = enshwick.content.add_knowledge({ id = "you.trail:given", scope = "Player", kind = "Bool" })
local is_given = { id = given, scope = "Player", test = "IsSet" }
enshwick.content.add_quest({
    id = "you.trail:trail",
    name = enshwick.content.guid("you.trail:trail.name"),
    source = "None",
    kind = "PlayerQuest",
    steps = { {
        name = enshwick.content.guid("you.trail:trail.step"),
        text = enshwick.content.guid("you.trail:trail.step.text"),
        shown_when = is_given,
        done_when = is_given,
    } },
    rewards = { { Item = "<the GUID of a vanilla item>" } },
})
```

## Who needs the mod

The journal is drawn on the client from its own copy of the data, so a mod
that adds a quest or a page runs on both sides.

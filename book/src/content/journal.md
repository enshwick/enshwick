<!--
type: how-to
description: Mods add quests and lore pages, change vanilla quests and their rewards, and read the journal and where each player stands in it.
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
| `id` | the quest's [content id](../concepts/ids.md) |
| `name` | its title's text tag |
| `source` | who gives it, which groups it in the journal |
| `kind` | `PlayerQuest` or `WorldQuest`. The third kind is `Auto`. |
| `main` | whether the journal lists it as a main quest |
| `for_all_players` | whether one player's completion unlocks it for all |
| `steps` | its steps, in order |
| `rewards` | items by GUID and experience values |

Each `QuestStep` shows while `shown_when` holds. It is done once `done_when`
holds. Its `progress` lists parts the journal counts. A mod moves the quest on by
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
quests on their own rather than with the world.
`knowledge.set_quest_progression(player, personal)` sets it, as the journal's
own setting does. The server sets it and the game sends it to the clients.

These calls need the `typed` feature.

## Reading the journal and where a player stands

`load.quests()` gives the `Quests` of the running game. They read the
journal as the game built it, every mod's quests and changes in, on first
use:

| Call | What it gives |
| --- | --- |
| `all()` | every quest, with its steps, their queries and its rewards |
| `quest(id)` | one quest, or none |
| `pages()` | every lore page and its entries |
| `state(player, id)` | per step whether it shows and is done, and whether the quest started, is done and was paid |
| `world_state(id)` | the same from the world's knowledge alone |

A step can ask one of the game's own queries (`KnowledgeTest::Holds` or
`Fails`). It then takes the game's answer, the one the game reads when it
pays a quest. Before the game has evaluated the query, the step reads as
unknown.

```rust
fn report(load: &mut enshwick::Load<'_>, player: enshwick::Entity) {
    let quests = load.quests();
    for quest in quests.all() {
        if quests.state(player, quest.id).is_some_and(|state| state.done) {
            enshwick::info!("quest {} done", quest.id);
        }
    }
}
```

## From Luau

Three of the same calls run at the top level of `main.luau`:

- `enshwick.content.add_quest{...}`
- `enshwick.content.add_journal_page{...}`
- `enshwick.content.change_quest(id, {...})`

Two more run while the world runs:

- `enshwick.knowledge.quest_progression(player)`
- `set_quest_progression(player, personal)`

The journal reads run under `enshwick.quests`, with the names of the Rust
calls.

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

The client draws the journal from its own copy of the data. So a mod that
adds a quest or a page runs on both sides.

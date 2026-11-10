<!--
type: how-to
description: Mods add quests and lore pages, change vanilla quests and their rewards, and read the journal and where each player stands in it.
-->

# Quests and journal pages

A mod adds quests and lore pages to the journal and changes the game's
quests. A quest's steps show and complete on [knowledge](knowledge.md). The
game gives its experience; the mod gives its items. The game's screens
draw them.

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
| `rewards` | the items by GUID and experience values the journal shows as its reward |

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

## Giving a quest's items

The game gives a quest's experience and never its items. It only draws an
item reward in the journal. The rewards of its own quests name NPCs,
workshops and chest icons. This is read in client 26430625 and not yet
seen in the game. So a mod gives the items of its own quest when `QuestCompleted`
names it. `give_what_fits` gives as many as the inventory has room for. It
tells the handler how many were not given and the request's outcome. A
committed outcome left them for want of room; any other gave none. The mod
decides what becomes of
those: tell the player, or keep them in its own
[world data](../state.md) and give them later. Enshwick stores nothing
about a reward.

```rust
use enshwick::events::QuestCompleted;
use enshwick::knowledge::KnowledgeId;

fn init(load: &mut enshwick::Load<'_>, trail: KnowledgeId, torch: enshwick::ItemId) -> Result<(), enshwick::Off> {
    let inventories = load.inventories()?;
    load.on::<QuestCompleted>(move |done| {
        if done.quest == trail {
            let _ticket = inventories.give_what_fits(done.entity, torch, 1, |left, outcome| {
                if !outcome.is_committed() {
                    enshwick::warn!("reward not given: {:?}", outcome.done);
                } else if left > 0 {
                    enshwick::warn!("no room for {left} of the reward");
                }
            });
        }
    })
}
```

`load.quests().quest(done.quest)` gives the rewards the quest declares, for
a handler that serves several quests. A mod gives only the items it
declared itself. A vanilla quest's own item reward is a picture, not an
item to hand out.

`QuestCompleted` is raised for every change that sets the quest's bool.
The game sets it once, but a mod that grants it again raises the event
again. A mod that must give only once keeps a mark in its world data.

A data pack without a script gives on `quest = true` in
[`rewards.toml`](rewards.md), for every quest a player completes. It gives
as many items as fit and logs the rest; the player gets no message.

## A lore page

`load.add_journal_page(NewJournalPage)` adds a lore page. Each `PageEntry`
shows once its `shown_when` holds. A player finds the page in the journal
(J) under "World information", on the card of its category. The card counts
the page from the start and lists it once one entry shows.

`category` is the text tag of one of the game's own lore categories. The
journal has a card only for those, so `add_journal_page` refuses any other
tag and logs the categories of the running game. A category of the mod's
own is not possible.

| Card | Constant in `journal::lore_category` | Text tag |
| --- | --- | --- |
| General | `GENERAL` | `f94a5ece-d3fb-4f32-8d6e-e72bdffaba39` |
| Ancients and the Flame | `ANCIENTS_AND_THE_FLAME` | `a86a5728-f02a-4e28-b0f5-1d53d69f7261` |
| The Shroud, Elixir, and Elixir Wells | `THE_SHROUD_ELIXIR_AND_ELIXIR_WELLS` | `8ab85ee5-5709-4ff7-b5cf-d60954cafec0` |
| About Scavengers | `ABOUT_SCAVENGERS` | `04123d11-4918-4fd9-853f-682e39ee8b50` |
| Vukah and Wildlife | `VUKAH_AND_WILDLIFE` | `e1a327ed-419c-4cef-9ccf-0d8bf142db0a` |
| Nobles and History | `NOBLES_AND_HISTORY` | `58b2bc1e-713e-4b0a-9225-71df283e1c4f` |
| Revelwoods | `REVELWOODS` | `771bb6b6-3ce5-4bfc-bb1a-7e1a1a2c9d2b` |
| Nomad Highlands | `NOMAD_HIGHLANDS` | `2e66d970-04a3-4078-b3bf-3e33d4996728` |
| Kindlewastes | `KINDLEWASTES` | `a57cfea9-74fc-42d0-8a12-b05a2ed112b9` |
| The Hollow | `THE_HOLLOW` | `1e15a1ec-ecc1-4f6c-afb2-9eb3609dbdac` |
| Cold Heights | `COLD_HEIGHTS` | `c811b8f1-bd2b-4e31-80a3-65a54ff794d9` |
| Veilwater Basin | `VEILWATER_BASIN` | `437ca83f-c2ec-4feb-9095-108c627a6ac7` |
| Drak | `DRAK` | `99e69e04-0fbc-4317-a9e3-1623261a4c0e` |

The cards and their English texts are those of client 26430625. The
journals of that client and of server 26430253 list their pages under these
13 tags. The game ships no name for a category, so the constants take the
cards' texts. They and `add_journal_page` need the `unstable` feature until
a mod's page is seen on its card in the game. A Luau mod passes the text tag.

## Changing a vanilla quest

`load.change_quest(quest, QuestChange)` changes a vanilla quest in memory:

- `rewards` replaces its rewards, and `add_rewards` adds after them;
- `for_all_players` sets whether one player's completion unlocks it for all;
- `steps` replaces the knowledge a step shows or completes on.

Mods that change one quest change it in load order, each on the one before's
result. `knowledge.quests()` lists the quest ids of the running game.

The game gives the experience of an added reward and only draws an added
item. A mod gives that item itself on `QuestCompleted`, as for a quest of
its own (see [Giving a quest's items](#giving-a-quests-items)).

## Personal quest progression

`knowledge.quest_progression(player)` says whether a player follows the
quests on their own rather than with the world.
`knowledge.set_quest_progression(player, personal)` sets it, as the journal's
own setting does. The server sets it and the game sends it to the clients.

These calls need the `typed` feature.

## Reading the journal and where a player stands

The `enshwick::quests` module reads the journal. `load.quests()` gives the
`Quests` of the running game, the same for every call of one mod. On first use they read the journal as the game's first
read builds it, every mod's quests and changes in:

| Call | What it gives |
| --- | --- |
| `all()` | every quest, with its steps, their queries and its rewards |
| `quest(id)` | one quest, or none |
| `pages()` | every lore page and its entries |
| `state(player, id)` | per step whether it shows and is done, and whether the quest started, is done and was paid |
| `world_state(id)` | the same from the world's knowledge alone |

A step can ask one of the game's own queries (`KnowledgeTest::Holds` or
`Fails`). It then takes the game's answer, the one the game reads when it
pays a quest. A step reads as unknown before the game has evaluated its
query. It also reads as unknown for a form enshwick does not read
(`KnowledgeTest::Unknown`) and for a player not in the game.

```rust
use enshwick::quests::Quests;

fn report(quests: &Quests, player: enshwick::Entity) {
    for quest in quests.all() {
        if quests.state(player, quest.id).is_some_and(|state| state.rewarded) {
            enshwick::info!("quest {} done, {} steps", quest.id, quest.steps.len());
        }
    }
}
```

```luau
local function report(player: number)
    for _, quest in enshwick.quests.all() do
        local state = enshwick.quests.state(player, quest.id)
        if state and state.rewarded then
            enshwick.log.info(`quest {quest.id} done, {#quest.steps} steps`)
        end
    end
end
```

## From Luau

Three of the same calls run at the top level of `main.luau`:

- `enshwick.content.add_quest{...}`
- `enshwick.content.add_journal_page{...}`
- `enshwick.content.change_quest(id, {...})`

`enshwick.on("QuestCompleted", handler)` subscribes there too, and
`enshwick.inventory.give_what_fits(to, item, count, done)` gives the items.

Two more run while the world runs:

- `enshwick.knowledge.quest_progression(player)`
- `set_quest_progression(player, personal)`

The journal reads run under `enshwick.quests`, with the names of the Rust
calls.

```luau
local given = enshwick.content.add_knowledge({ id = "you.trail:given", scope = "Player", kind = "Bool" })
local is_given = { id = given, scope = "Player", test = "IsSet" }
local trail = enshwick.content.add_quest({
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

local torch = enshwick.content.item_id("<the GUID of a vanilla item>")
enshwick.on("QuestCompleted", function(done)
    if done.quest == trail then
        enshwick.inventory.give_what_fits(done.entity, torch, 1, function(left, outcome)
            if outcome.done ~= "Committed" then
                enshwick.log.warn(`reward not given: {outcome.done}`)
            elseif left > 0 then
                enshwick.log.warn(`no room for {left} of the reward`)
            end
        end)
    end
end)
```

## Who needs the mod

The client draws the journal from its own copy of the data. So a mod that
adds a quest or a page runs on both sides.

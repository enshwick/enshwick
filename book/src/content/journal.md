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
holds. Its `progress` lists parts the journal counts. A query left `None`, as
`QuestStep::owned(name, text)` leaves both, is the loader's:

- a step without `done_when` gets a bool of its own;
- a step without `shown_when` shows once the step before it is done;
- the first step without `shown_when` shows once a bool of its own is set.

These bools are each player's for a player's quest and the world's for a
world quest. A mod moves the quest on with the calls of
[Moving a quest on](#moving-a-quest-on), or by granting the knowledge its
steps ask for. Texts come from the mod's texts, in every language it gives
(see [Texts](texts.md)).

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

## Following a quest

The loader raises three events from the knowledge that quest steps test,
for the game's quests and mods' quests alike:

| Event | When |
| --- | --- |
| `QuestStarted` | the quest's first step shows now and did not before |
| `QuestStepDone` | a step's completion holds now and did not before |
| `QuestProgressed` | a counter that a step's counted part reads changed |

A game query decides some steps, and the game answers a query in its own
pass. So the loader asks such a step again for three steps after an entry
that its query reads changes. A step whose game query reads no knowledge,
such as one on items, raises nothing. What a player holds when they join
raises nothing. The entity is 0 when the knowledge that moved is the
world's.

```rust
use enshwick::events::{QuestProgressed, QuestStepDone};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on::<QuestStepDone>(|done| {
        enshwick::info!("player {} did step {} of quest {}", done.entity.0, done.step, done.quest);
    })?;
    load.on::<QuestProgressed>(|counted| {
        enshwick::info!("{} of {}", counted.value, counted.target);
    })
}
```

```lua
enshwick.on("QuestStepDone", function(done)
    enshwick.log.info(`player {done.entity} did step {done.step} of quest {done.quest}`)
end)
```

## Changing what a quest pays

The game pays a quest's experience values and sets the quest's bool.
`load.on_quest_reward` runs before it pays. The filter keeps the payment,
pays other experience values, or refuses. A refused quest pays nothing and
still completes. The same value twice pays it twice. The game pays at most
`limit` values, 3 on server 26430253, and the loader drops the rest with a
warning. Filters run in load order, and each one sees the payment as the
filters before it left it. `allowed` is `false` once a filter refused
it. Other values leave a refusal standing, and `QuestPay::Allow` pays a
refused quest again. A payment that the game puts off comes again in a
later step, and the filters see it again.

```rust
use enshwick::quests::QuestPay;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_quest_reward(|paid| {
        QuestPay::Pay(paid.experience.iter().flat_map(|&value| [value, value]).collect())
    })
}
```

```lua
enshwick.quests.on_quest_reward(function(paid)
    local twice = {}
    for _, value in paid.experience do
        table.insert(twice, value)
        table.insert(twice, value)
    end
    return twice
end)
```

A Luau filter returns `false` to refuse and `true` to pay a refused quest
again. It returns a list to pay those values, and nil to keep the payment.

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

`QuestCompleted` can come again for a quest the player already completed,
as the end of this section says. So the handler keeps a mark per player in
a world store and gives only on the first one.

```rust
use std::collections::BTreeSet;

use enshwick::events::QuestCompleted;
use enshwick::knowledge::KnowledgeId;

/// The players given the reward, by account key in hex.
#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Given {
    players: BTreeSet<String>,
}

fn init(load: &mut enshwick::Load<'_>, trail: KnowledgeId, torch: enshwick::ItemId) -> Result<(), enshwick::Off> {
    let inventories = load.inventories()?;
    let world = load.world()?;
    let given = load.world_store::<Given>("torch")?;
    load.on::<QuestCompleted>(move |done| {
        if done.quest != trail {
            return;
        }
        let Some(player) = world.player(done.entity) else {
            return;
        };
        let key = format!("{:x}", player.key.get());
        // `None` while no world runs or the store is shut: no reward then.
        if given.with(|given| given.players.contains(&key)) != Some(false) {
            return;
        }
        given.update(|given| given.players.insert(key.clone()));
        let given = given.clone();
        let _ticket = inventories.give_what_fits(done.entity, torch, 1, move |left, outcome| {
            if outcome.is_committed() && left == 0 {
                return;
            }
            // Nothing was given: the next completion tries again.
            given.update(|given| given.players.remove(&key));
            if outcome.is_committed() {
                enshwick::warn!("no room for the reward");
            } else {
                enshwick::warn!("reward not given: {:?}", outcome.done);
            }
        });
    })
}
```

A request that is not committed, or that found no room for the one item,
gave nothing. The handler clears the player's mark then, so the next
completion tries again.

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

`load.change_quest(quest, QuestChange)` changes a vanilla quest in memory.
Each field left `None` keeps what the quest holds:

| Field | What it sets |
| --- | --- |
| `name`, `document` | the title's and the document's text tags |
| `source`, `kind`, `main` | who gives it, whose it is, whether it is a main quest |
| `for_all_players` | whether one player's completion unlocks it for all |
| `priority`, `tutorial` | where the journal sorts it, whether it is the tutorial's |
| `rewards`, `add_rewards` | its rewards, and rewards after them |
| `steps` | `StepEdit`s, applied in order |

A `StepEdit` changes, inserts, removes or moves one step, each on the steps
the edits before it left:

- `Change(StepChange)` sets a step's title, text, queries, `progress`, map
  marker, voiceover, icon or level; a text tag of zeros clears the marker
  and the voiceover;
- `Insert(StepInsert { at, step })` inserts a step, which names both its
  queries, with an entry id no other entry of the journal holds;
- `Remove(index)` and `Move(StepMove { from, to })`.

A step's `marker` is the text tag that a map marker of `ui::map` carries.
The journal's "show on map" then points at that marker. A mod's own quest
takes the same fields:

| Type | Fields |
| --- | --- |
| `QuestStep` | `marker`, `voiceover` |
| `NewQuest` | `priority`, `tutorial`, `document` |

`load.remove_quest(quest)` removes a vanilla quest with its steps. Its
knowledge stays, so a save with its progress still opens. The loader logs
and skips a removal while another mod's quest asks for it or one of its steps.
A step asks by the id or through a game query that reads it.

Lore pages need the `unstable` feature:

- `load.change_journal_page(page, PageChange)` changes a vanilla page the
  way `change_quest` changes a quest. An inserted entry needs only its
  `shown_when`;
- `load.remove_journal_page(page)` removes one.

`Quests::pages` lists each page's `EntryId`.

Mods that change one quest change it in load order, each on the one before's
result. `knowledge.quests()` lists the quest ids of the running game.

The game gives the experience of an added reward and only draws an added
item. A mod gives that item itself on `QuestCompleted`, as for a quest of
its own (see [Giving a quest's items](#giving-a-quests-items)).

## An item set

An item set is a collection on the journal's collection screen. Each entry
is an item to find, and the set gives experience once every entry is done.
`Load::add_item_set` adds one to a category of the game's:

```rust
use enshwick::ItemId;
use enshwick::content::id;
use enshwick::content::journal::{ItemSetEntry, NewItemSet, item_set_category};

fn init(load: &mut enshwick::Load<'_>, shells: [ItemId; 2]) -> Result<(), enshwick::Off> {
    load.add_item_set(NewItemSet::new(
        id!("shells"),
        item_set_category::COLLECTIONS,
        id!("shells.name").guid(),
        shells.map(ItemSetEntry::new).to_vec(),
    ))?;
    Ok(())
}
```

| Field | What it is |
| --- | --- |
| `category` | one of the game's categories; any other is refused |
| `entries` | `ItemSetEntry`: the item, its `done_when`, a recipe the screen names, and hints |
| `experience` | what the game gives once every entry is done |
| `recipe_rewards` | the recipes the screen lists as the set's reward |
| `shown_when` | when the screen shows the set; `None` for once one entry is done |

The journal holds a category as the key of its title text.
`journal::item_set_category` names the game's 20 categories in English,
from client 426's texts, such as `COLLECTIONS`. Luau passes the key, such as `3331860567` for
"Collections", and `quests.item_sets()` gives each set's key.

An entry's `done_when` left `None` is done once a player first picks its
item up. That is the item's pickup flag, as the game's own entries ask.
The loader adds two queries to the game's query database for the set, as
the game's own sets have them. One asks all of the entries, for the set's
completion. The other asks any of them, for showing the set (see
[A query of the mod's own](knowledge.md#a-query-of-the-mods-own)).

The game's server pays a set as it pays its own. It walks the pay rows, at
most 50 a run. A player whose set holds and who lacks the set's bool gets
the experience, and the game sets the bool. So a set pays once. This is
read in the code of server 26430253. Whether the game grants the recipes
is not read yet, so a mod that wants them granted grants them itself.

`Load::change_item_set` changes a vanilla set: its title, experience,
recipes, entries and queries, and its pay row with them. New entries need
the set's `done_when` too, since the game's completion query asks for the
old ones. `Load::remove_item_set` removes a vanilla set with its pay row.
`quests.item_sets()` reads every set of the running game, with its entries,
queries and pay row's bool.

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

A step can ask one of the game's own queries (`KnowledgeTest::Holds`). It
then takes the game's answer, the one the game reads when it
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

## Moving a quest on

`Quests` moves a quest on for a `Holder`. A holder is
`Holder::Player(player)` or `Holder::World`. Each call grants or revokes the knowledge the steps test,
vanilla quests and mods' alike. The game's knowledge is the quest's only
state, so a save, a rejoin and a vanilla client agree with it.

| Call | What it does |
| --- | --- |
| `step(holder, id)` | the step the holder stands at: the first not done, or the number of steps once all are |
| `start(holder, id)` | grants what the first step's `shown_when` asks, so the journal lists the quest |
| `advance(holder, id)` | finishes the step the holder stands at and shows the next; returns the next step |
| `complete(holder, id)` | finishes every step left |
| `reset(holder, id)` | clears the holder's entries the steps test and, for a player, the quest's bool |

A grant goes the game's way and applies at the next step of the world. One
call grants each entry once, even when two steps test it. A step on a game
query, `KnowledgeTest::Holds`, is refused. The error is
`MoveQuestError::Unwritable`, since the game computes that query from its
parts. The mod grants those parts itself. A test no grant reaches, such as a count
below a value, is refused the same way.

The game pays a quest itself. Its server checks each player's quests in
turn, at most 50 a run. It pays the experience of a quest whose last step
holds and whose bool is unset, then sets the bool. So a completed quest
pays once, and a reset quest pays again on its next completion. This is
read in the code of server 26430253 and not yet seen in the game. What the
grants brought, such as experience, stays after a reset. A reset clears an
entry for everything that tests it, such as another quest or a recipe.

```rust
use enshwick::quests::{Holder, MoveQuestError, Quests};

fn finish(quests: &Quests, player: enshwick::Entity, quest: enshwick::knowledge::KnowledgeId) {
    match quests.complete(Holder::Player(player), quest) {
        Ok(()) => enshwick::info!("quest {quest} completes at the next step"),
        Err(MoveQuestError::Unwritable { step, .. }) => {
            enshwick::warn!("step {step} moves only through its game query");
        }
        Err(other) => enshwick::warn!("quest {quest}: {other}"),
    }
}
```

```luau
local function finish(player: number, quest: number)
    local ok, err = pcall(enshwick.quests.complete, { Player = player }, quest)
    if not ok then
        enshwick.log.warn(`quest {quest}: {err}`)
    end
end
```

These calls need the `typed` feature, as the journal reads do.

## Objectives the journal counts

A step's `objectives` list what the journal counts on it. The loader gives
each objective a counter of its own. It adds the count to the step's
`progress`, so the journal shows how far the player is.

| Objective | What counts one |
| --- | --- |
| `Collect { item, count }` | a pickup of the item, up to what the inventory then holds |
| `Kill { target, count }` | a kill of the template or faction, by the player the game credits |
| `Craft { target, count }` | a unit of the recipe, as a craft reward counts it |
| `Build { item, count }` | the item placed, undo and redo left out |
| `Reach { at, radius }` | the player within the radius, checked once a second |
| `Custom { signal, count }` | the amount `quests.signal(holder, signal, amount)` sends |

A step of one objective and no `done_when` is done once the count is
reached. A step of several gets a bool of its own instead. The loader sets
that bool once every count is reached. When the last two counts are reached
in the same step of the world, it sets the bool within a second.

A kind of objective the running build cannot count is logged when the
quest is added, and only that kind stays at 0.

A counter rises only while its step shows and is not done. A player's quest
counts that player's actions. A world quest counts every player's.
`CraftTarget::Item` counts the vanilla recipes that make the item; a mod's
own recipe counts through `CraftTarget::Recipe`.

The loader subscribes once per kind of event, not per objective. An event
first looks its target up in one table: about 4 ns with 100 objectives on a
desktop of 2026.

```rust
use enshwick::content::id;
use enshwick::content::journal::{NewQuest, Objective, QuestStep};
use enshwick::quests::{Holder, Quests};

fn init(load: &mut enshwick::Load<'_>, wall: enshwick::ItemId) -> Result<(), enshwick::Off> {
    let step = QuestStep {
        objectives: vec![
            Objective::Build { item: wall, count: 4 },
            Objective::Custom { signal: id!("beacon-lit"), count: 1 },
        ],
        ..QuestStep::owned(id!("fort.build").guid(), id!("fort.build.text").guid())
    };
    load.add_quest(NewQuest::new(id!("fort"), id!("fort.name").guid(), vec![step]))?;
    Ok(())
}

fn lit(quests: &Quests, player: enshwick::Entity) {
    quests.signal(Holder::Player(player), id!("beacon-lit"), 1);
}
```

```luau
local step = {
    name = enshwick.content.guid("you.fort:fort.build"),
    text = enshwick.content.guid("you.fort:fort.build.text"),
    objectives = {
        { Build = { item = 2577, count = 4 } },
        { Custom = { signal = "you.fort:beacon-lit", count = 1 } },
    },
}
enshwick.content.add_quest({
    id = "you.fort:fort",
    name = enshwick.content.guid("you.fort:fort.name"),
    source = "None",
    kind = "PlayerQuest",
    steps = { step },
})
```

Objectives need the `typed` feature.

## From Luau

The same calls run at the top level of `main.luau`:

- `enshwick.content.add_quest{...}`
- `enshwick.content.add_journal_page{...}`
- `enshwick.content.change_quest(id, {...})`
- `enshwick.content.remove_quest(id)`
- `enshwick.content.change_journal_page(page, {...})` and
  `enshwick.content.remove_journal_page(page)`
- `enshwick.content.add_item_set{...}`
- `enshwick.content.change_item_set(set, {...})` and
  `enshwick.content.remove_item_set(set)`

A step edit is a table with one key:

| Edit | Luau |
| --- | --- |
| change | `{ Change = { step = 0, level = 9 } }` |
| insert | `{ Insert = { at = 0, step = {...} } }` |
| remove | `{ Remove = 1 }` |
| move | `{ Move = { from = 0, to = 2 } }` |

`enshwick.on("QuestCompleted", handler)` subscribes there too, and
`enshwick.inventory.give_what_fits(to, item, count, done)` gives the items.

Two more run while the world runs:

- `enshwick.knowledge.quest_progression(player)`
- `set_quest_progression(player, personal)`

The journal reads and the calls that move a quest on run under
`enshwick.quests`, with the names of the Rust calls. A holder is
`"World"` or `{ Player = player }`. A step's `shown_when` and `done_when`
are optional, as in Rust.

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
-- The players given the reward, by account key: a mark against a repeat.
local rewarded = enshwick.world_store("torch")
local function mark(key: string, set: boolean)
    rewarded:update(function(data)
        local marks = if type(data) == "table" then data else {}
        marks[key] = set -- false, not nil: an empty table saves as none
        return marks
    end)
end
enshwick.on("QuestCompleted", function(done)
    if done.quest ~= trail then
        return
    end
    local player = enshwick.world.player(done.entity)
    if not player then
        return
    end
    local key = player.key
    -- nil while no world runs or the store is shut: no reward then.
    local marked = rewarded:with(function(data)
        return type(data) == "table" and data[key] == true
    end)
    if marked ~= false then
        return
    end
    mark(key, true)
    enshwick.inventory.give_what_fits(done.entity, torch, 1, function(left, outcome)
        if outcome.done == "Committed" and left == 0 then
            return
        end
        -- Nothing was given: the next completion tries again.
        mark(key, false)
        if outcome.done == "Committed" then
            enshwick.log.warn("no room for the reward")
        else
            enshwick.log.warn(`reward not given: {outcome.done}`)
        end
    end)
end)
```

## Who needs the mod

The client draws the journal from its own copy of the data. So a mod that
adds a quest, a page or an item set runs on both sides.

# Knowledge and quests

[`load.knowledge()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/knowledge.rs)
returns the
[`Knowledge`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/knowledge.rs)
of the server's world: the values the game keeps for the world and for each
player, which recipes, quests, journal pages, skill points and much more
read. It is `Copy`; get it during `init` and move it into handlers, tasks and
commands. It needs ABI minor 32.

```rust
use enshwick_loader_api::events::QuestCompleted;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let knowledge = load.knowledge()?;
    let world = load.world()?;
    load.on::<QuestCompleted>(move |done| {
        let entity = enshwick_loader_api::ecs::Entity(done.entity);
        let total = knowledge.quests().into_iter().filter(|&q| knowledge.completed(entity, q)).count();
        if let Some(player) = world.player(entity) {
            world.broadcast(&format!("{} completed a quest ({total} so far)", player.name));
        }
    })
}
```

## Ids and values

Every knowledge id is the world's, shared by every player, or each player's
own, and is a bool, a counter or flags. `get(player, id)` reads a player's
value (for an id of the world's, the world's) and `world(id)` the world's, as
a `Known`: the kind, the scope, the value, and the bits the game expects of
flags. `is_set()` and `has_flag(bit)` read it. Both answer `None` for an id
the game does not know, a player not in the game, or off the game thread.

Some ids mean something fixed:

| Knowledge | Id | Value |
| --- | --- | --- |
| a recipe | the recipe's id | player flags: bit 0 seen, bit 1 crafted |
| a quest | the quest's id, from `quests()` | player bool, set when the game rewards the quest |
| a recipe's unlock | what the recipe's requirement asks for | usually world flags or bools |

The game ships no names for its knowledge ids; a mod that names them keeps
its own table, built from the game's data.

## Granting

`grant(player, id, grant)` and `grant_world(id, grant)` grant knowledge as the
game does: the next step applies it with everything that follows, such as
experience, unlocks chained on it, the clients and the save. The grant must
fit the id's kind: `Grant::Set` for a bool, `Grant::Add(n)` for a counter,
`Grant::Flag(bit)` for one bit of flags. A grant only adds; the game has no
way to take knowledge back.

A grant fails with a [`WorldError`](world.md#writing-components): `Refused`
for an id the game does not know, a grant that does not fit the kind, a bit
the game does not expect, a player not in the game, or a step that already
holds the game's limit of grants; `WrongThread` off the game thread. Granting a quest's or a recipe's own id skips what leads to it, so
the journal may show a quest done without its steps; granting the knowledge
its requirement asks for keeps it whole.

## Events

[`KnowledgeChanged`](events.md) is raised on the game thread after each step
for every value that step changed, with the player's character (0 for the
world), the id and the new value; [`QuestCompleted`](events.md) when a
player completed a quest and got its rewards, with the character and the
quest's id.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.

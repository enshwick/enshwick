# Knowledge and quests

[`load.knowledge()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/knowledge.rs)
returns the
[`Knowledge`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/knowledge.rs)
of the server's world: the values the game keeps for the world and for each
player, which recipes, quests, journal pages, skill points and much more
read. It is `Copy`; get it during `init` and move it into handlers, tasks and
commands.

```rust
use enshwick::events::QuestCompleted;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let knowledge = load.knowledge();
    let world = load.world()?;
    load.on::<QuestCompleted>(move |done| {
        let entity = enshwick::ecs::Entity(done.entity);
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
`Grant::Flag(bit)` for one bit of flags. A grant only adds; a revoke takes
back.

A grant fails with a [`WorldError`](world.md#writing-components): `Refused`
for an id the game does not know, a grant that does not fit the kind, a bit
the game does not expect, a player not in the game, or a step that already
holds the game's limit of grants; `WrongThread` off the game thread. Granting a quest's or a recipe's own id skips what leads to it, so
the journal may show a quest done without its steps; granting the knowledge
its requirement asks for keeps it whole.

## Taking back

`revoke(player, id, revoke)` and `revoke_world(id, revoke)` take
knowledge back: `Revoke::Clear` clears a bool, `Revoke::Sub(n)` lowers a
counter down to 0, `Revoke::Unflag(bit)` clears one bit of flags. The value
changes at once, the clients get it again (the world's to every client, a
player's to that player), and the next save keeps it; `KnowledgeChanged`
reports it after the next step. What the grant brought stays: experience,
unlocks chained on it, skill points already spent. A quest's id taken back
while its steps still hold is rewarded again. A revoke fails as a grant
does, and for a player whose save has not loaded yet.

## Players not in the game

A player's knowledge lives in the character's save on the player's machine;
the server holds it only while the player is in the game, once that save has
loaded. `grant_account(account, id, grant)` and
`revoke_account(account, id, revoke)` take the account's
[`Player::key`](world.md) instead of a character: when the account's
character is in the game the change applies now, otherwise it waits with the
world, in `mod-data/enshwick/worlds/<key>/knowledge.toml`, and applies once
the account's next character has loaded in that world, whichever character
it is. Only a player's ids qualify.

```rust
use enshwick::knowledge::Grant;

fn reward(knowledge: &enshwick::knowledge::Knowledge, account: u64, quest_reward: u32) {
    if let Err(why) = knowledge.grant_account(account, quest_reward, Grant::Set) {
        enshwick::warn!("no reward: {why}");
    }
}
```

## What clients ask for

A client asks the server for knowledge in its input, for what its interface
marks, and a vanilla server grants whatever it asks: a modified client could
grant itself any recipe, quest or skill point. The loader's knowledge guard
knows what a vanilla client asks for, a player's bools and the seen bit (0)
of a player's flags; the rest is ids of the world's, counters, other flag
bits, ids that give experience or skill points, and quests. `knowledge_guard`
in `mod-configs/enshwick.toml` says what it does with the rest:

| Value       | The guard                                                            |
| ----------- | -------------------------------------------------------------------- |
| `"watch"`   | the default: logs each request it would refuse and lets it through   |
| `"enforce"` | refuses it; a refused request changes nothing and is not asked again |
| `"off"`     | does nothing; mods' filters still run                                |

Requests the guard would refuse or refuses, and those a filter refuses, are
logged as `knowledge guard: would refuse, let through ...` or
`knowledge guard: refused ...`, the first 64 of a run; the status summary
counts them all.

`load.on_knowledge_input(filter)` runs `filter` on every request
after the guard, with the guard's reason (`refusal`, given in every mode) and
the verdict so far (`allowed`); its answer replaces the verdict, so a mod can
refuse more or allow what the guard refuses. A filter's refusal stands in
every mode; a watching guard refuses nothing itself, so filters see its
requests allowed. Filters run in load order on the server's worker threads,
only when a client asks.

```rust
use enshwick::knowledge::Refusal;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_knowledge_input(|request| request.allowed && request.refusal != Some(Refusal::FlagBit))
}
```

## Events

[`KnowledgeChanged`](events.md) is raised on the game thread after each step
for every value that step changed, with the player's character (0 for the
world), the id and the new value; [`QuestCompleted`](events.md) when a
player completed a quest and got its rewards, with the character and the
quest's id.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.

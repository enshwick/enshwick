# Rewards for actions

A reward says "when a player does this, give that". A mod declares it in
code or in a pack's `rewards.toml`; the server gives it and keeps who got
it beside the world's save. The calls are [unstable](../stability.md).

## Declare a reward

A reward has a name, the action it follows, what it gives, a limit and a
chance.

```rust
use enshwick::rewards::{Action, Give, Limit, Reward};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    const TONIC: enshwick::ItemId = enshwick::ItemId::named("you.tonics:tonic");
    load.add_reward(
        Reward::new("level-two", Action::LevelUp(2))
            .with(Give::Items { item: TONIC, count: 2 })
            .with(Give::Experience(50))
            .with_limit(Limit::Once),
    )
}
```

```luau
local tonic = enshwick.content.item_id("you.tonics:tonic")
enshwick.rewards.add_reward({
    name = "level-two",
    on = { LevelUp = 2 },
    give = { { Items = { item = tonic, count = 2 } }, { Experience = 50 } },
    limit = "Once",
})
```

```toml
# rewards.toml
[[reward]]
id = "level-two"
on = { level_up = 2 }
give = [{ item = "tonic", count = 2 }, { experience = 50 }]
limit = "once"
```

In `rewards.toml` an item or a recipe is the pack's own name, another mod's
content id or a vanilla name, as in [the other pack files](packs.md); a
knowledge entry or a buff is its GUID.

| Action             | `on` in `rewards.toml`  | When                                       |
| ------------------ | ----------------------- | ------------------------------------------ |
| `Craft(recipe)`    | `craft = "<recipe>"`    | Each unit the server crafts for the player |
| `LevelUp(level)`   | `level_up = <level>`    | The player reaches the level; 0 for any    |
| `Join`             | `join = true`           | The player joins the world                 |
| `Loot`             | `loot = true`           | The player loots a container               |
| `Quest`            | `quest = true`          | The player completes a quest               |
| `Consume(item)`    | `consume = "<item>"`    | The player's use of the item goes through  |
| `Pickup(item)`     | `pickup = "<item>"`     | The player picks the item up               |

| Gift                       | In `rewards.toml`                 | What the player gets                                     |
| -------------------------- | --------------------------------- | -------------------------------------------------------- |
| `Items { item, count }`    | `{ item = "<item>", count = 2 }`  | The items, all or nothing; a full inventory gets none   |
| `Experience(amount)`       | `{ experience = 50 }`             | Experience on the next gains, at most a level a tick    |
| `Knowledge(id)`            | `{ knowledge = "<guid>" }`        | The knowledge entry, set                                 |
| `Buff(id)`                 | `{ buff = "<guid>" }`             | The buff, for its type's own duration                    |

The limit is `Every` (`every`, when left out), `Once` per player in the
world, `Daily`, once per player each calendar day in UTC, or `World`, once
in the world to the first player. The chance is from 0 to 1, always when
left out. `pack check` reports a reward without an action or with two, one
that gives nothing, a chance outside 0 to 1, a name used twice and a name
the pack does not define.

## Hold a grant back or give from code

[`load.on_reward(filter)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/rewards.rs)
runs before the mod gives any of its rewards; `false` holds the grant back
and leaves its limit unused. To give something else, give it and return
`false`. `load.rewards()` returns the handle that grants a declared reward
by name, within its limit and chance, or gives anything at once.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_reward(|grant| grant.reward != "level-two" || grant.player != enshwick::Entity(1))?;
    let rewards = load.rewards()?;
    load.on::<enshwick::events::PlayerJoined>(move |joined| {
        let _given = rewards.grant(joined.entity, "level-two");
    })
}
```

```luau
enshwick.rewards.on_reward(function(grant)
    return grant.reward ~= "level-two" or grant.player ~= 1
end)
enshwick.on("PlayerJoined", function(player)
    enshwick.rewards.grant(player.entity, "level-two")
end)
```

## Where rewards run and what they keep

Rewards run in the server's world, on the game thread, a step after their
action. A craft counts for each unit the crafting system made of the
player's request, so a request for three gives the reward three times. The
game crafts a request in the step after it and reports the units in the
step after that; the server pairs each unit with the oldest request of its
recipe from the step before, up to the amount asked. A unit with no such
request, such as what a station makes on its own, gives nothing, and a
request the game refused gives nothing later. When two players ask for the
same recipe in one step and the first runs short of ingredients, units of
the second player's request may count for the first. The
world's file `rewards.toml` in the mod's data folder beside the save keeps
who got a limited reward, by account, so a limit holds across restarts and
moves with the save. Items and experience are given as the reward states
them; the game's rates do not change them.

A reward whose action this game build lacks turns off alone, with a
warning in the log; the rest of the pack applies. `Rewards::grant` and
`Rewards::give` run on the thread that calls them, so call them on the game
thread; an item handler runs on its system's thread, so schedule the grant
from there.

<!--
type: how-to
description: A mod refuses a craft, a placement or a player's interaction with an entity before the game acts on it. A filter that panics turns the mod off and lets it pass.
-->

# Crafting, building and interaction vetoes

A mod may refuse a craft, a placement or a player's interaction with an
entity before the game acts on it. Each filter returns `false` to refuse; one
that panics turns the mod off and lets the action through.

## Crafting

[`load.on_craft(|craft| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/actions/src/vetoes.rs)
runs on each player's craft request in the server's world. It gets the game's
`CraftRecipeEvent`, with these fields:

- `crafting_operator_id`;
- `crafting_station_id`, `None` for crafting by hand;
- `recipe_id`;
- `recipe_amount`.

Returning `false` refuses it: nothing is made and nothing is taken.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    // Only administrators craft by hand.
    load.on_craft(move |craft| {
        craft.crafting_station_id.is_some()
            || world.player(craft.crafting_operator_id).is_some_and(|p| p.is_admin())
    })
}
```

The loader screens the requests on the game thread at the start of the next
step. That comes before the crafting system reads them, so [`World`](world.md) reads work in the filter. A refused
request reaches the crafting system empty, with no crafter and no amount.

## Building

[`load.on_build(|placement| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/actions/src/vetoes.rs)
runs after the game's cursor check allows a placement. It gets a `Placement`,
a name of Enshwick's own, since the game's check has none. Its fields are
these:

- `player`, who builds;
- `item`, what is placed;
- `position`, where, in metres.

It answers a `BuildVerdict`:

- `Refuse` refuses the placement the way the game refuses one without
  building permission, so the building systems do not place it.
- `RefuseBecause` refuses it with a reason the builder reads.

The reason is a key of the mod's
[texts](https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/i18n.rs),
shown in the player's language, or plain words. A client that runs the mod
shows it under the build cursor while the player aims there. Some clients do
not show it. The server tells such a player once, as a notice, or in chat to a
client without Enshwick. It tells one player at most once a second. It tells
the same reason again only after that player's refusals stopped for a second.
A `bool` turns into a verdict with `.into()`.

```rust
use enshwick::vetoes::BuildVerdict;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // Nothing is built in a ring around the world's centre.
    load.on_build(|placement| {
        let [x, _, z] = placement.position;
        if (x * x + z * z).sqrt() > 50.0 {
            BuildVerdict::Allow
        } else {
            BuildVerdict::refuse_because("build.centre")
        }
    })
}
```

```luau
-- Nothing is built in a ring around the world's centre.
enshwick.vetoes.on_build(function(placement)
    local at = placement.position
    if math.sqrt(at.x * at.x + at.z * at.z) > 50 then
        return true
    end
    return "build.centre"
end)
```

From Luau the filter returns `true` or nil to allow, and `false` to refuse.
It returns a string to refuse with that reason. Any other value refuses too.

The server runs the cursor check on the cursor each player's client sends.
The client runs it for its own prediction. So the filter runs on both sides,
on the cursor systems' worker threads, on every frame a player aims to build.
Keep it quick: no allocation, no lock, no log, and no world reads, which
belong on the game thread.

The server's answer decides. A client that runs the mod too shows the cursor
as not allowed, so give the same answer on both sides. Base it on what both
know, the item and the place, or on
[settings](settings.md#on-a-joined-server) the server sends. A client without
the mod shows the placement as allowed, and the server places nothing.

## Interactions

[`load.on_interaction`](https://github.com/enshwick/enshwick/blob/main/crates/internal/actions/src/interactions.rs)
runs a filter before the game lets a player interact with an entity, from
opening a chest to dismantling a piece. It gets an `Interaction`, which holds:

- what the player does (`kind`);
- the player;
- the target;
- the item the target was placed from and where it stands, when the
  platform could read them.

Returning `false` refuses it.

```rust
use enshwick::interactions::Kind;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    const LOCKED_CHEST: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
    load.on_interaction(|i| !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move | Kind::Sort)))
}
```

```luau
local LOCKED_CHEST = 0x5e01
enshwick.interactions.on_interaction(function(i)
    local guarded = i.kind == "Open" or i.kind == "Move" or i.kind == "Sort"
    return not (i.item == LOCKED_CHEST and guarded)
end)
```

| Kind         | When it is asked                                          | What a refusal does              |
| ------------ | --------------------------------------------------------- | -------------------------------- |
| `Open`       | The game is about to offer the Open prompt                | No prompt                        |
| `Move`       | Items move out of or into the target, or one of its slots | The move is undone on the client |
| `Sort`       | The target's inventory is sorted                          | Nothing is sorted                |
| `Stock`      | The target lends its stock to crafting or building nearby | It is left out of the stock      |
| `Dismantle`  | The cursor offers to pick up or tear down the target      | No pick-up or tear-down          |
| `QuickStash` | The player flips the target's Quick Stash switch          | The switch stays                 |
| `Upgrade`    | The player upgrades the target into another piece         | Nothing is upgraded              |

Each refusal takes the game's own path for a refused action, so nothing moves
and nothing is lost. `Stock` is asked without the target's item or position:
answer it from what the mod has already seen of the target.

An interaction without a player's character or without a target is allowed
without asking. The filters run in the server's world only, in the dedicated
server or a hosting client. They run on the thread of the system that checks
the interaction, so keep them quick and never block. Several mods may filter; any
refusal wins.

### What a player's prompt offers now

With the `unstable` feature, `load.interaction_targets()` returns the
players' prompt targets in the server's world. Its `target(player)` returns
the entity whose interaction the player's prompt offers now. It returns
`None` while the prompt offers nothing, and for an entity that is no player.

It reads the target the game keeps for each player, so a player without
Enshwick reads the same. It runs on the server's game thread between steps,
as `held` does in [the controls a player holds](input.md). The client
chooses what its prompt shows, so check what matters on the server.

```rust
use std::time::Duration;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let targets = load.interaction_targets()?;
    let world = load.world()?;
    load.on_tick(Duration::from_secs(1), move |_| {
        for player in world.players() {
            if let Ok(Some(target)) = targets.target(player.entity) {
                enshwick::info!("{} looks at {target:?}", player.name);
            }
        }
    })?;
    Ok(())
}
```

```luau
enshwick.on_tick(1, function()
    for _, player in enshwick.world.players() do
        local target = enshwick.interactions.target(player.entity)
        if target ~= nil then
            enshwick.log.info(`{player.name} looks at {target}`)
        end
    end
end)
```

## Worked example

[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
links its interaction filter in
[`lib.rs`](https://github.com/enshwick/mods/blob/main/personal-safes/src/lib.rs).
Its
[`access.rs`](https://github.com/enshwick/mods/blob/main/personal-safes/src/access.rs)
decides who may open, move and dismantle a Safe.

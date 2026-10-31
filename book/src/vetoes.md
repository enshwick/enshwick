# Crafting, building and interaction vetoes

A mod may refuse a craft, a placement or a player's interaction with an
entity before the game acts on it. Each is a filter that returns `false` to
refuse. A filter that panics turns the mod off and lets the action through.

## Crafting

[`load.on_craft(|craft| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/vetoes.rs)
runs on each player's craft request in the server's world, with the game's
`CraftRecipeEvent`: the `crafting_operator_id`, the `crafting_station_id`
(`None` for crafting by hand), the `recipe_id` and the `recipe_amount`.
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

The loader screens the requests
on the game thread at the start of the next step, before the crafting system
reads them, so [`World`](world.md) reads work in the filter. A refused
request reaches the crafting system empty, with no crafter and no amount.

## Building

[`load.on_build(|placement| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/vetoes.rs)
runs after the game's cursor check allows a placement, with a `Placement`,
Enshwick's name, since the game's check has none: the building `player`,
the `item` placed and its `position` in metres.
Returning `false` refuses it the way the game refuses a placement without
building permission: the building systems do not place it.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // Nothing is built in a ring around the world's centre.
    load.on_build(|placement| {
        let [x, _, z] = placement.position;
        (x * x + z * z).sqrt() > 50.0
    })
}
```

The server runs the cursor check on the cursor each player's client sends,
and the client runs it for its own prediction, so the filter runs on both
sides, on the cursor systems' worker threads, on every frame a player aims to
build. Keep it quick: no allocation, no lock, no log, and no world reads,
which belong on the game thread. The server's answer decides. A client that
runs the mod too shows the cursor as not allowed, so give the same answer on
both sides: base it on what both know, the item and the place, or on
[join values](settings.md#join-values) the server shares. A client without the
mod shows the placement as allowed, and the server places nothing.

## Interactions

[`load.on_interaction`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/interactions.rs)
runs a filter before the game lets a player interact with an entity, from
opening a chest to dismantling a piece. It gets an `Interaction`: what the
player does (`kind`), the player, the target, and the item the target was
placed from and where it stands, when the platform could read them.
Returning `false` refuses it.

```rust
use enshwick::interactions::Kind;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    const LOCKED_CHEST: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
    load.on_interaction(|i| !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move | Kind::Sort)))
}
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
server or a hosting client, on the thread of the system that checks the
interaction: keep them quick and never block. Several mods may filter; any
refusal wins.

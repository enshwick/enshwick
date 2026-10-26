# Crafting and building vetoes

A mod may refuse a craft or a placement before the game acts on it.
[Interactions](interactions.md) with entities, from opening a chest to
dismantling a piece, have their own filter. A filter that panics turns the mod off and lets the action
through.

## Crafting

[`load.on_craft(|craft| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/vetoes.rs)
runs on each player's craft request in the server's world, with a
`CraftRequest`: the crafting `player`, the `station` (`None` for crafting by
hand), the `recipe` id and the `amount`. Returning `false` refuses it:
nothing is made and nothing is taken.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    // Only administrators craft by hand.
    load.on_craft(move |craft| {
        craft.station.is_some() || world.player(craft.player).is_some_and(|p| p.is_admin())
    })
}
```

A request is the game's `CraftRecipeEvent`. The loader screens the requests
on the game thread at the start of the next step, before the crafting system
reads them, so [`World`](world.md) reads work in the filter. A refused
request reaches the crafting system empty, with no crafter and no amount.

## Building

[`load.on_build(|placement| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/vetoes.rs)
runs after the game's cursor check allows a placement, with a `Placement`:
the building `player`, the `item` placed and its `position` in metres.
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

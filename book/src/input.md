<!--
type: reference
description: A server mod reads which of the game's controls each player holds this step, as their client sent them, and what it may trust of that.
-->

# The controls a player holds

[`load.input()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/input.rs)
returns the
[`Input`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/input.rs)
of the server's world: which of the game's controls each player holds this
step. It is unstable: it needs the `unstable` feature (see
[Stability](stability.md)).

## Reading what a player holds

`input.held(player)` returns the game actions that player holds this step, as
an `InputMask`. The mask has these calls:

- `contains(GameAction)`;
- `iter()`;
- the raw `bits()`.

It reads the player's own input in the server's world, as their client last
sent it. For that reason it reads a player without Enshwick the same way. It
returns `None` for an entity that is no player.

`Input` is `Copy`. Get it during `init` and move it into ticks and handlers.
`held` runs on the server's game thread between steps, such as in a tick.
Elsewhere it fails with a [`WorldError`](world.md):

- `WrongThread` off the game thread or inside a step;
- `Missing` before a world runs.

```rust
use std::time::Duration;

use enshwick::controls::GameAction;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let input = load.input()?;
    let world = load.world()?;
    load.on_tick(Duration::from_secs(1), move |_| {
        for player in world.players() {
            if let Ok(Some(held)) = input.held(player.entity)
                && held.contains(GameAction::Block)
            {
                enshwick::info!("{} blocks", player.name);
            }
        }
    })?;
    Ok(())
}
```

```luau
enshwick.on_tick(1, function()
    for _, player in enshwick.world.players() do
        local held = enshwick.input.held(player.entity)
        if held ~= nil and table.find(held, "Block") then
            enshwick.log.info(`{player.name} blocks`)
        end
    end
end)
```

In Luau, `held` returns the list of the held actions' game names. They are the
names a [control's](controls.md) `link` uses, such as `"ContextualAction_Tap"`.

## Which bit is which

The game keeps the held controls as a 64-bit mask in each player's
`PlayerInput`. The running build's reflection names each bit, and the
loader maps each name to its `GameAction`. A bit the build does not name,
or that `GameAction` lacks, shows in `bits()` only.

## What to trust

The held controls are what the player's client sent, and a client can send
any of them, as often as it likes. Read them for what the player could do
with the keys anyway, such as a stance or a hint. Never let them decide
rights, rewards or an admin step. Those come from the server's own state,
or from a [player action](actions.md), which the server checks on every
run.

<!--
type: how-to
description: Read the containers a dead enemy leaves, corpses and loot bags, and other containers that dissolve once emptied. Their items are emptied through inventories.
-->

# Loot containers

[`enshwick::loot`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/loot.rs)
reads the containers a dead enemy leaves, corpses and loot bags, and other
containers that dissolve once emptied. It reads only; empty a container
through [Inventories](../inventories.md). To put items into the loot a
container rolls, see [Random loot](random-loot.md).

## Each check of the containers

The game checks every such container each step in one ECS system,
[`loot::SYSTEM`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/loot.rs)
(`destroy_inventory_on_loot`), on a worker thread.
[`load.on_loot_pass`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/loot.rs)
runs a handler after each run of it in the server's world. The handler gets a
`LootPass` that walks that run's containers and reads any entity as one:

```rust
use enshwick::world::near;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_loot_pass(|pass| {
        let spot = [0.0, 0.0, 0.0];
        let mut open = 0;
        pass.each(|c| {
            if c.is_open() && near(spot, c.position, 12.0, 4.0) {
                open += 1;
            }
        });
        let _ = open;
    })
}
```

```luau
local spot = vector.create(0, 0, 0)

enshwick.loot.on_loot_pass(function(containers)
    local open = 0
    for _, c in containers do
        local offset = c.position - spot
        local across = vector.magnitude(vector.create(offset.x, 0, offset.z))
        local enemy_loot = c.expires and c.area_loot == true
        if enemy_loot and not c.is_empty and c.offer and not c.offer.is_disabled
            and across <= 12 and math.abs(offset.y) <= 4 then
            open += 1
        end
    end
end)
```

A Luau handler gets the pass's containers as a list of `Container` tables.
Their fields are those of the table under
[Reading one container](#reading-one-container). The Luau example spells out
the checks `is_open()` and `near`.

`each` leaves the system's row where it found it, so the walk does not
disturb vanilla. Keep the handler short; it runs every step. A mod that walks
another system with the same reads uses the unsafe `Containers` from that
system's own hook; see [ECS inside systems](../ecs.md).

## Reading one container

[`Container`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/loot.rs)
holds what one container is and holds, read in one go:

| Field | Meaning |
| --- | --- |
| `entity` | The container. |
| `position` | Where it stands, in metres; y is up. |
| `is_empty` | Whether it holds nothing, as the game last recorded. |
| `expires` | Whether it despawns on its own after a while (`LifeTime`). |
| `area_loot` | Its `AreaLootOverride`, or `None` without one. `Some(true)` means area loot may take it. `Some(false)` means it is excluded, as items a player dropped are. |
| `offer` | Its interaction offer, an [`InteractionOfferRuntime`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/loot.rs); `None` without one. |

An `InteractionOfferRuntime` has three fields:

- `guest_id`: who last opened or looted the container, `None` before anyone
  did;
- `last_accept_time`: when, in the game's nanoseconds, changing with each
  interaction;
- `is_disabled`: whether the game offers no interaction now.

Two methods sort containers:

- `is_enemy_loot()`: it expires and is open to area loot. That keeps corpses
  and enemy loot bags, and leaves out player chests, world chests, quest
  containers and items a player dropped;
- `is_open()`: enemy loot that also holds something and offers its
  interaction, so a player could take from it now.

## Other entities

From inside any hooked system, with its context:

- `container(ctx, entity)` reads one entity as a loot container. It gives
  `None` when the entity is gone or is not one: it has no `DestroyOnLoot`,
  inventory state or position;
- `position(ctx, entity)` gives any entity's position in metres;
- `is_player(ctx, entity)` tells whether an entity is a player character,
  such as an offer's `guest_id`.

[`near(center, at, radius, height)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/world/geometry.rs)
tells whether `at` lies near the centre. It must lie within `radius` metres
of the centre across the ground, and within `height` metres above or below
it:

```rust
use enshwick::world::near;

assert!(near([0.0, 0.0, 0.0], [6.0, 3.0, 8.0], 10.0, 4.0));
assert!(!near([0.0, 0.0, 0.0], [6.0, 5.0, 8.0], 10.0, 4.0));
```

## On a hosting client

A client that hosts a game runs the system in the built-in server's world
and in its own. `on_loot_pass` calls its handler for the server's world only,
checked with
[`Worlds::is_server(ctx)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/ecs.rs),
so the handler needs no check of its own. Entity ids of the two worlds are
unrelated. See [Sides and hosting](../concepts/sides-and-hosting.md).

A server-side loot mod declares `side: SERVER`; the client loads `SERVER`
mods too, for the games it hosts.

## A worked example

A mod that empties the loot around a container a player just emptied into
the same player puts the pieces of this chapter together. The steps follow
one such mod's
[`take.rs`](https://github.com/enshwick/mods/blob/main/area-loot/src/take.rs):

1. At `init` it hooks `loot::SYSTEM` and resolves these:
   - `Containers`
   - `Inventories`
   - `Worlds`
2. The hook calls vanilla first, then skips any context for which
   `is_server` is false, and walks the containers with `each`. It catches a
   panic in its own pass and turns itself off, so the game keeps running
   vanilla.
3. A tracker remembers each container's state between steps. A container
   that went empty after a player's interaction (the offer's `guest_id`, checked
   with `is_player`) starts a take.
4. It gathers the containers around that are `is_open()` and `near` within
   the configured radius and height. It leaves out any that another player
   opened or looted in the last 10 seconds. It also leaves out any whose last
   guest, another player, still stands within 4 metres.
5. It empties them one at a time with a `MoveAll` batch, nearest first. It
   goes on to the next only when the batch committed. A
   [read](../inventories.md#reads) in the callback must also show the last
   one empty.
   A full inventory stops it.
6. On `ShutdownStarted` it clears the tracker: entity ids belong to one game.

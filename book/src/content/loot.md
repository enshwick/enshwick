# Loot containers

[`enshwick_loader_api::loot`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/loot.rs)
reads the containers a dead enemy leaves, corpses and loot bags, and other
containers that dissolve once emptied. It reads only; empty a container
through [Inventories](../inventories.md). To put items into the loot a
container rolls, see [Random loot](random-loot.md).

## Each check of the containers

The game checks every such container each step in one ECS system,
[`loot::SYSTEM`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/loot.rs)
(`destroy_inventory_on_loot`), on a worker thread.
[`load.on_loot_pass`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/loot.rs)
runs a handler after each run of it in the server's world, with a `LootPass`
that walks that run's containers and reads any entity as one:

```rust
use enshwick_loader_api::loot::near;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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

`each` leaves the system's row where it found it, so the walk does not
disturb vanilla. Keep the handler short; it runs every step. A mod that walks
another system with the same reads uses the unsafe `Containers` from that
system's own hook; see [ECS inside systems](../ecs.md).

## Reading one container

[`Container`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/loot.rs)
holds what one container is and holds, read in one go:

| Field | Meaning |
| --- | --- |
| `entity` | The container. |
| `position` | Where it stands, in metres; y is up. |
| `empty` | Whether it holds nothing, as the game last recorded. |
| `expires` | Whether it despawns on its own after a while (`LifeTime`). |
| `area_loot` | Its `AreaLootOverride`: `Some(true)` when area loot may take it, `Some(false)` when excluded (items a player dropped), `None` without one. |
| `offer` | Its interaction offer, an [`Offer`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/loot.rs); `None` without one. |

An `Offer` names who last opened or looted the container (`guest`,
`Entity::NONE` before anyone did), when (`accepted_at`, in the game's
nanoseconds, changing with each interaction) and whether the game offers no
interaction now (`disabled`).

Two methods sort containers:

- `is_enemy_loot()`: it expires and is open to area loot. That keeps corpses
  and enemy loot bags, and leaves out player chests, world chests, quest
  containers and items a player dropped;
- `is_open()`: enemy loot that also holds something and offers its
  interaction, so a player could take from it now.

## Other entities

From inside any hooked system, with its context:

- `container(ctx, entity)` reads one entity as a loot container; `None` when
  it is not one (no `DestroyOnLoot`, inventory state or position) or is
  gone;
- `position(ctx, entity)` gives any entity's position in metres;
- `is_player(ctx, entity)` tells whether an entity is a player character,
  such as an offer's `guest`.

[`near(center, at, radius, height)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/loot.rs)
tells whether `at` lies within `radius` metres of `center` across the
ground and within `height` metres above or below it:

```rust
use enshwick_loader_api::loot::near;

assert!(near([0.0, 0.0, 0.0], [6.0, 3.0, 8.0], 10.0, 4.0));
assert!(!near([0.0, 0.0, 0.0], [6.0, 5.0, 8.0], 10.0, 4.0));
```

## On a hosting client

A client that hosts a game runs the system in the built-in server's world
and in its own. Check
[`Worlds::is_server(ctx)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/ecs.rs)
first, as the example does: it is always true on the dedicated server, and on
a client only for the built-in server's systems. Entity ids of the two
worlds are unrelated. See [Sides and hosting](../concepts/sides-and-hosting.md).

A server-side loot mod declares `side: SERVER`; the client loads `SERVER`
mods too, for the games it hosts.

## A worked example: area loot

[`enshwick-area-loot`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-area-loot)
empties the loot around a container a player just emptied into the same
player. Its
[`take.rs`](https://github.com/themaiby/enshwick/blob/main/mods/enshwick-area-loot/src/take.rs)
puts the pieces of this chapter together:

1. At `init` it resolves `Containers`, `Inventories` and `Worlds`, and hooks
   `loot::SYSTEM`.
2. The hook calls vanilla first, then skips any context for which
   `is_server` is false, and walks the containers with `each`. It catches a
   panic in its own pass and turns itself off, so the game keeps running
   vanilla.
3. A tracker remembers each container's state between steps. A container
   that went empty after a player's interaction (the offer's `guest`, checked
   with `is_player`) starts a take.
4. It gathers the containers around that are `is_open()` and `near` within
   the configured radius and height, and leaves out any that another player
   opened or looted in the last 10 seconds, or whose last guest, another
   player, still stands within 4 metres.
5. It empties them one at a time with a `MoveAll` batch, nearest first, and
   goes on to the next only when the batch committed and a
   [read](../inventories.md#reads) in the callback shows the last one empty.
   A full inventory stops it.
6. On `Shutdown` it clears the tracker: entity ids belong to one game.

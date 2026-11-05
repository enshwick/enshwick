<!--
type: explanation
description: Enshrouded ships a dedicated server and a client, and the Enshwick loader runs in both. A mod's side says where it runs: on a server or while a client hosts.
-->

# Sides and hosting

Enshrouded ships two binaries: `enshrouded_server.exe`, the dedicated server,
and `enshrouded.exe`, the client. The loader runs in both.

## A mod's side

`side` in the manifest says where a mod runs. Both binaries load every mod;
the side says who must have it:

- `server`: the mod runs only where the world runs, on the dedicated server
  or on a client that hosts a game, solo play included. Players need not
  install it.
- `both`: the mod runs there and on every client playing in the world, such
  as content or a value the client's interface also reads. Every player in
  the world must have it in the world's version; see
  [The join check](join-check.md).

A mod for the client alone does not exist: the loader refuses any other side.
A client runs only the mods the world it plays in runs, and turns its others
off for that world.

## Hosting

A client that hosts a game, solo play included, runs the whole server on a
thread of its own, `GameServer`, beside its own world. Every mod must work
there as on a dedicated server, so the server features act whenever the
client hosts:

- game events, the world reads, the scheduler, inventories and actions
  serve the built-in server from its first step until the game stops;
- each hosted game starts with `WorldLoaded` and ends with `ShutdownStarted`. The next
  game has a new server, a new thread and new entities, so reset state tied to
  one game, such as entity ids, on `ShutdownStarted`;
- while no game is hosted, the server features rest: the world reads return
  nothing and no game events come.

[`Host::hosting`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/host.rs)
tells whether this process runs a server now: always on the dedicated server,
on the client from the built-in server's first step until the game stops.

```rust
fn acts() -> bool {
    enshwick::host().is_some_and(|host| host.hosting())
}
```

## Two worlds in one process

Server systems run only in the server's world, but some game functions serve
both worlds of a hosting client, such as cursor checks and base stock. A hook
on one of them acts for the server only after it asks
[`ecs::Worlds`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs):

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let worlds = enshwick::ecs::Worlds::resolve(load)?;
    // In a hooked system, with its context `ctx`:
    // if unsafe { worlds.is_server(ctx) } { ... }
#   let _ = worlds;
    Ok(())
}
```

`is_server(ctx)` is always true on the dedicated server. Entity ids of the two
worlds are unrelated. [ECS inside systems](../ecs.md) has more.

## Symbols on the client

The client binary holds its own copies of the server's functions, at other
addresses. A mod that pins server symbols itself pins them for the client too,
in a build row with `side = "client"`, the binary the row is for. When a mod's server and client halves hook
the same function, one link answers for both. See
[Finding the game](../finding-the-game.md).

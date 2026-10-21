# Buffs

[`load.buffs()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/buffs.rs)
returns the
[`Buffs`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/buffs.rs)
of the server's world: the buff and debuff types the game knows, the buffs an
entity has, giving one and removing one. It is `Copy`; get it during `init`
and move it into handlers, tasks and commands. It needs ABI minor 31.

```rust
use core::time::Duration;
use enshwick_loader_api::buffs::{Apply, BuffId};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let buffs = load.buffs()?;
    load.add_command("buffs", "lists your buffs", move |ctx, _args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        match buffs.of(me) {
            Ok(on) => on
                .iter()
                .map(|b| format!("{:#x} {:?}, {}s left", b.id.0, b.kind, b.remaining.as_secs()))
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
            Err(why) => format!("{why}").into(),
        }
    })?;
    load.add_command("bless", "a buff for five minutes: /bless <buff id>", move |ctx, args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        let Some(id) = args.first().and_then(|a| a.parse().ok()).map(BuffId) else {
            return "/bless <buff id>".into();
        };
        format!("{:?}", buffs.apply(me, id, Apply::new().lasting(Duration::from_mins(5)))).into()
    })
}
```

## Types

A buff type is named by its `BuffId`: the FNV-1a 32 hash of the type's GUID,
as the game's content names it; `BuffId::from_guid(bytes)` makes one, and
`BuffId::named("my-mod:warmth")` makes the id of a buff type a mod serves
under a [content id](content.md#content-ids).
`types()` lists every type of the running game (274 on the 1.0 game) with
its `BuffKind` (`Food`, `Buff`, `Debuff`, `Rested` or `Terrain`), its
default duration, whether the game saves it with a character, whether it
shows, and the `LocaTag` of its name, which a client shows. The dedicated
server has no texts of its own, so a mod that names buffs keeps its own
table or reads the names on a client.

## An entity's buffs

`of(entity)` lists the buffs an entity has: the type, the buff's own entity
(which carries its effect), the kind, the level when it has one, its whole
duration and the time left, in game time. `has(entity, id)` asks for one
type. Characters, survivors, animals and enemies hold buffs; anything else
answers `WorldError::Lacks`.

## Giving and removing

`apply(entity, id, how)` gives a buff at once, as the game restores a
character's saved buffs when it joins: `Apply::new()` with the type's own
duration, `.lasting(duration)` for another, `.level(n)` for a level. A buff
the entity has already is renewed to the duration asked and keeps its level.
The game's own faction and death checks do not run: the mod decides who gets
a buff.

`remove(entity, id)` removes every buff of the type, as the game cleanses
one: it goes in the next step, without what the type does when it runs out,
and the call returns whether the entity had it. The game frees the buff's
place only in that next step, so giving the same type again in the step it
was removed answers `WorldError::Refused`: give it from a
[scheduler](events.md#the-scheduler) task one step later.

Clients see buffs given and removed as the game's own, with their icons and
times. The game saves the types it saves with a character, buffs from mods
among them; other types end when the player leaves.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.

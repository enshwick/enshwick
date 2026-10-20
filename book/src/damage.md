# Damage

[`load.on_damage(filter)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/damage.rs)
runs a filter on damage about to reach an entity's health in the server's
world, before the game applies it. The filter gets a
[`Damage`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/damage.rs)
to read and change; what it leaves is what the game applies. It needs ABI
minor 30.

```rust
use enshwick_loader_api::damage::{DamageCause, DamageType};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on_damage(|damage| match damage.cause() {
        DamageCause::Fall => damage.cancel(),
        DamageCause::Hit => damage.scale_type(DamageType::Poison, 0.5),
        _ => {}
    })
}
```

## Causes

| `cause()` | What deals it | What the filter changes |
| --- | --- | --- |
| `Hit` | a weapon, a spell, a creature, an explosion | each damage type |
| `Fall` | a fall | the amount |
| `Shroud` | the shroud, once a character's time in it ran out | the amount |
| `ShroudDeath` | the deadly shroud, which kills at once | the amount: the whole health, unless the filter lowers it |

A hit's damage comes by type, after the target's resistances and before its
armour, so the armour, a block and the hit the game raises (and a
[`Hit` game event](game-events.md)) follow what the filters leave. The types
are the game's: `Cut`, `Pierce`, `Blunt`, `Fire`, `Ice`, `Shroud`,
`Lightning`, `Healing`, `Poison`, `Wood`, `Stone`, `Metal`, `Explosion`.
`Healing` is what the hit heals; `scale` and `cancel` leave it alone.

`target()` is the entity that takes the damage; `source()` is what dealt a
hit, the attacker or its projectile, and `None` for the other causes.
`amount(ty)` and `set(ty, amount)` read and set one type of a hit; `total()`
is the sum but healing, or the amount of another cause. `scale(factor)` scales
everything, `scale_type(ty, factor)` one type, and `cancel()` takes it all
away: the health stays, and a hit still lands with no damage, as against an
immune target. A negative or not finite amount counts as none.

Filters of several mods run in load order, each on what the one before left.

## Where it runs

The filter runs on the game's worker threads, in parallel, inside the
systems that deal damage, in the dedicated server or a hosting client;
remote clients run none of it and show the server's numbers. Keep it short:
no allocation, no lock, no logging; read only what the mod holds itself, such
as settings kept in atomics. A filter that panics leaves the damage as it
was and turns the mod off.

Damage the game deals by other ways, such as drowning, freezing or an
administrator's kill, does not pass the filter, and neither does mining,
which wears terrain and buildings by volume rather than health.

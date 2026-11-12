<!--
type: how-to
description: A damage filter runs on damage about to reach an entity's health in the server's world. It reads the cause and changes or cancels the damage before it lands.
-->

# Damage

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

[`load.on_damage(filter)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/inventory/src/damage.rs)
runs a filter on damage about to reach an entity's health in the server's
world, before the game applies it.

## Filtering damage

The filter gets a
[`Damage`](https://github.com/enshwick/enshwick/blob/main/crates/internal/inventory/src/damage.rs)
to read and change; what it leaves is what the game applies.

```rust
use enshwick::damage::{DamageCause, DamageTypeNew};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_damage(|damage| match damage.cause() {
        DamageCause::FallDamage => damage.cancel(),
        DamageCause::Impact => damage.scale_type(DamageTypeNew::Poison, 0.5),
        _ => {}
    })
}
```

```luau
enshwick.damage.on_damage(function(damage)
    local cause = damage:cause()
    if cause == "FallDamage" then
        damage:cancel()
    elseif cause == "Impact" then
        damage:scale_type("Poison", 0.5)
    end
end)
```

## Causes

| `cause()` | What deals it | What the filter changes |
| --- | --- | --- |
| `Impact` | a weapon, a spell, a creature, an explosion | each damage type |
| `FallDamage` | a fall | the amount |
| `Fog` | the shroud, once a character's time in it ran out | the amount |
| `DeadlyFog` | the deadly shroud, which kills at once | the amount: the whole health, unless the filter lowers it |
| `Suffocated` | a character out of breath under water | the amount |
| `DrowningDeath` | a creature out of breath under water, which dies at once | the amount: the whole health, unless the filter lowers it |
| `Freezing` | a character frozen through, out of body heat | the amount |
| `AdminKill` | an administrator's kill of chosen enemies, or of whole factions or players | only whether: the amount is 1, and 0 refuses the kill |

A hit's damage comes by type, after the target's resistances and before its
armour. So the armour, a block and the hit the game raises (and a
[`Hit` game event](game-events.md)) follow what the filters leave. The types
are the game's, every value of `DamageTypeNew`:

- `PhysicalCut`
- `PhysicalPierce`
- `PhysicalBlunt`
- `MagicalFire`
- `MagicalIce`
- `MagicalFog`
- `MagicalLightning`
- `Healing`
- `Poison`
- `Wood`
- `Stone`
- `Metal`
- `Explosion`

`Healing` is what the hit heals. `scale` and `cancel` leave it alone.

`target()` is the entity that takes the damage. `source()` is what dealt a
hit: the attacker or its projectile. `attacker()` is who dealt it, such as the
archer for an arrow. Both are `None` for the other causes.
`amount(ty)` and `set(ty, amount)` read and set one type of a hit. `total()`
is the sum but healing, or the amount of another cause. `scale(factor)` scales
everything, and `scale_type(ty, factor)` one type. `cancel()` takes it all
away: the health stays, and a hit still lands with no damage, as against an
immune target. A negative or not finite amount counts as none.

Filters of several mods run in load order, each on what the one before left.

## Where it runs

The filter runs on the game's worker threads, in parallel, inside the
systems that deal damage. It runs in the dedicated server or a hosting client.
Remote clients run none of it and show the server's numbers. Keep it short:
no allocation, no lock, no logging; read only what the mod holds itself, such
as settings kept in atomics. A filter that panics leaves the damage as it
was and turns the mod off.

Executions are hits and pass as `Impact`. An administrator's kill zeroes every
health value of its target at once, so a filter can only let it through or
refuse it whole. Three things do not pass the filter:

- health the game's scripts set or heal
- an entity whose lifetime ends
- mining, which wears terrain and buildings by volume rather than health

# Character progress

Three typed hooks cover how characters grow, from
[`progress`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/progress.rs).

## Skills

`load.on_skill(node, handler)` answers the game's checks of one skill node.
The handler gets a `SkillCheck` with the node, the `SkillGate` that asks and
the game's own answer, and returns `Some(true)` to unlock the node for that
check, `Some(false)` to lock it, or `None` for the game's answer. The platform
names the gates it knows, such as `SkillGate::FlyMode`, so a mod can open one
feature without touching the skill tree. It runs on both sides.

```rust
use enshwick_loader_api::progress::{SkillGate, AIRBORNE_ARCHITECT};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on_skill(AIRBORNE_ARCHITECT, |check| (check.gate == SkillGate::FlyMode).then_some(true))
}
```

## Comfort

Each tick every comfort provider in a base registers its comfort for its
category; the base keeps the best per category and a player's comfort is the
sum. `load.on_comfort(filter)` returns the value each registers. Filters of
several mods run in load order, each on the last one's value. It runs on the
server's worker threads for every base: keep it to arithmetic.

## Experience

`load.on_experience(filter)` sees every spawned character's experience before
the server applies it: level, progress inside the level, what the level needs
and the gained experience the game applies next, one `Experience` per
character. Changes to `gained` are
written back and sent to the players. `Levels` gives the level cap and what
each level needs.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on_experience(|rows, levels| {
        for row in rows.iter_mut().filter(|row| row.level < levels.cap()) {
            row.gained = row.gained.saturating_add(1);
        }
    })
}
```

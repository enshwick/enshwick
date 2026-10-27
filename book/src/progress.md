# Character progress

Three typed hooks cover how characters grow, from
[`progress`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/progress.rs).

## Skills

`load.on_skill(skill, handler)` answers the game's checks of one `Skill` the
platform names; the loader finds its node on the running game build.
The handler gets a `SkillCheck` with the `SkillGate` that asks and the game's
own answer, and returns `Some(true)` to unlock the skill for that
check, `Some(false)` to lock it, or `None` for the game's answer. The platform
names the gates it knows, such as `SkillGate::FlyMode`, so a mod can open one
feature without touching the skill tree. It runs on both sides.

```rust
use enshwick::progress::{Skill, SkillGate};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_skill(Skill::AirborneArchitect, |check| (check.gate == SkillGate::FlyMode).then_some(true))
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
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_experience(|rows, levels| {
        for row in rows.iter_mut().filter(|row| row.level < levels.cap()) {
            row.gained = row.gained.saturating_add(1);
        }
    })
}
```

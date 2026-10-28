# Game rates

A game rate multiplies something the game hands out, such as experience or
workshop speed. A mod declares each
[`Rate`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/rates.rs)
it multiplies with `load.rate(rate)` during `init`. The call returns a
`RateHandle`, and its `set(factor)` puts the mod's factor in force: 1 is
vanilla, 2 doubles, 0 takes it all. Factors run from 0 to 100; workshop
speed keeps at least 0.01 and a loot stack at least 1.

```rust
use enshwick::rates::{ExperienceGroup, Rate};

#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default)]
struct Settings {
    /// Experience from defeated enemies, times this.
    #[config(runtime, range = 0.0..=100.0)]
    combat: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { combat: 1.0 }
    }
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let combat = load.rate(Rate::Experience(ExperienceGroup::Combat))?;
    let settings: Settings = load.config()?;
    combat.set(settings.combat)?;
    load.on_config_change(move |settings: Settings| {
        if let Err(why) = combat.set(settings.combat) {
            enshwick::warn!("combat experience unchanged: {why}");
        }
    })
}
```

## Rates

| Rate | What it multiplies |
| --- | --- |
| `Experience(Combat)` | experience from defeated enemies |
| `Experience(Mining)` | experience from mining and gathering |
| `Experience(Exploration)` | experience from exploration, lore, quests and item sets |
| `FactoryProductionSpeed` | how fast timed workshops such as the smelter and the kiln produce; 2 halves the time |
| `ResourceDropStackAmount` | how many resources a drop stacks, as the game's own setting does: mined, chopped and harvested nodes, and the random loot of chests, corpses and animals |
| `Comfort` | the comfort each piece of furniture gives a base, after every mod's comfort filter, rounded |
| `SkillPoints` | the skill points a character gains per level, rounded; points from knowledge stay |
| `Loot` | how many items each stack of random loot holds: chests, corpses, animals and random pickups; rounded, at least 1 and at most the item's stack size |
| `RecipeOutput(kind)` | how many items the recipes of each `RecipeKind` make, vanilla and mod recipes alike, after every mod's own recipe patch; rounded, at least 1 |

The experience, workshop and resource drop rates multiply the server's own
setting of the same name. Where `ResourceDropStackAmount` and `Loot` reach the same
roll, both apply. The loader
scales what the game's own functions answer in every process a mod declares
a rate in, and never writes the game's settings, so the server's
configuration and the host's world keep their own values. Experience,
drops, comfort and loot are decided where the world runs: on a dedicated
server, or on a client while it hosts a game.

## When several mods set a rate

The game uses the product of the factors of every mod that declared the
rate and is still on, held to 0 to 100. The rule is
[`combine`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/rates.rs),
the same in the loader and the fake host; `scale_whole` rounds a whole
number the game keeps. A product is the same in any
order, so load order never changes a rate. A mod that turns off drops out
of the product at once.

## Rates clients count

Clients count `FactoryProductionSpeed`, `SkillPoints` and `RecipeOutput` too, for
the time, the points and the counts their screens show, and the server checks every skill unlock against
its own count. Lowering `SkillPoints` mid-game leaves a character that spent
more than its new total unable to unlock more skills until it resets its
tree. Only a
mod with `join: true` may declare it, and `Rate::seen_by_clients` says which
rates need it. Set such a rate from a `sync` setting: every player who joins
then runs the mod with the server's factor, and a player without the mod
does not join ([the join check](concepts/join-check.md)).

## Recipe output

The loader applies the `RecipeOutput` rates in its own patch of the recipe
registry, after every mod's own recipe patch, so a mod that sets a recipe's
count sees it multiplied. The kinds are those of
[Recipe kinds](content/recipes.md#recipe-kinds). A dedicated server loads its items after
it first builds the registry, so the loader builds the registry again when a
world loads, and again whenever a mod changes a recipe output factor.

## Errors

`load.rate` answers `Off` on a game build that lacks what the rate needs,
and for a rate clients count in a mod without `join: true`. Carry on without
that rate with `.ok()` when the rest of the mod still makes sense. `set`
answers `RateError::OutOfRange` for a factor that is not finite or lies
outside 0 to 100.

## Testing

`FakeHost::without_rate(rate)` builds a game that lacks a rate, and
`Installed::rate(rate)` returns the factor the loader would put in force,
or `None` for a rate the mod did not declare.

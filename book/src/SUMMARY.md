# Summary

<!-- mdBook part titles are top-level headings. -->
<!-- markdownlint-disable MD025 -->

[Introduction](introduction.md)

- [Getting started](getting-started.md)
- [Concepts](concepts.md)
  - [The loader and the ABI](concepts/loader-and-abi.md)
  - [The life of a mod](concepts/lifecycle.md)
  - [Sides and hosting](concepts/sides-and-hosting.md)
  - [The join check](concepts/join-check.md)
  - [Threads](concepts/threads.md)
  - [Typed types across game builds](concepts/typed-stability.md)

# Writing a mod

- [Settings](settings.md)
- [Logging and diagnostics](logging.md)
- [Events and the scheduler](events.md)
- [Game events](game-events.md)
- [The world](world.md)
- [Effects](effects.md)
- [Time of day](daytime.md)
- [Game settings](game-settings.md)
- [Game rates](rates.md)
- [Weather and chunks](weather.md)
  - [Weather phenomena](weather-phenomena.md)
- [Terrain](terrain.md)
- [Altars and bases](altars.md)
- [The map](map.md)
- [Survivors](survivors.md)
- [Crafting, building and interaction vetoes](vetoes.md)
- [Damage](damage.md)
- [Character progress](progress.md)
- [Buffs](buffs.md)
- [Knowledge and quests](knowledge.md)
- [Player actions](actions.md)
- [Controls](controls.md)
- [Chat, notices and the console](chat.md)
- [Messages between server and client](messages.md)
- [The interface](interface.md)
- [State and data folders](state.md)
- [Services between mods](services.md)
- [Calls between mods](calls.md)
- [Extending other mods](extending.md)

# Writing a mod in Luau

- [Luau mods](luau.md)

# Reaching into the game

- [Finding the game](finding-the-game.md)
- [The game's own types](game-types.md)
- [Game resources as values](resource-values.md)
- [Hooks](hooks.md)
- [Mid-function hooks](mid-function-hooks.md)
- [Code patches](code-patches.md)
- [Code in the game's frame](game-frame.md)
- [Pacing and threads](pacing.md)
- [ECS inside systems](ecs.md)

# Content

- [Content](content.md)
  - [Items](content/items.md)
  - [What players do with items](content/item-actions.md)
  - [Rewards for actions](content/rewards.md)
  - [Recipes](content/recipes.md)
  - [Disabling recipes](content/disabled-recipes.md)
  - [Workshops](content/workshops.md)
  - [Entity templates](content/templates.md)
  - [Prop types](content/props.md)
  - [Enemies and arsenals](content/enemies.md)
  - [Spawners](content/spawns.md)
  - [Loot containers](content/loot.md)
  - [Random loot](content/random-loot.md)
  - [Served resources](content/resources.md)
  - [Texts](content/texts.md)
  - [A mod without Rust](content/a-mod-without-rust.md)
  - [Data packs](content/packs.md)
  - [Vanilla value changes from a mod](content/limits.md)
- [Inventories](inventories.md)
- [Item and entity data](item-and-entity-data.md)
- [Stats](stats.md)
- [Affixes](affixes.md)

# Shipping

- [Testing](testing.md)
- [Releasing mods](releasing.md)

# Reference

- [Stability](stability.md)
- [ABI reference](abi-reference.md)
- [Mods in C](mods-in-c.md)
- [FAQ and troubleshooting](faq.md)

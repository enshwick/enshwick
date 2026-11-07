<!--
type: how-to
description: What the enshwick crate keeps stable from 1.0 under Semantic Versioning, what the unstable feature holds, and how Luau mods and the ABI keep their versions.
-->

# Stability

From 1.0 the `enshwick` crate follows
[Semantic Versioning](https://semver.org/). So a mod built against 1.x still
builds against every later 1.y and loads in a 1.y loader.

## What is stable

Everything the crate offers without a feature:

- the mod and its loader: `Load`, `Host`, settings, texts and state;
- hooks, game memory, events and game events;
- the server's world calls outside `world`: altars, buffs, daytime, effects,
  game settings, the damage types, terrain, voxels and weather, whose errors share
  one type, `enshwick::WorldError`;
- inventories, stats and affixes, apart from writing them onto single items;
- content and data packs;
- services, mod events, extension points, messages and the interface.

## What `unstable` holds

Only the `unstable` feature stands outside that promise: what it holds may
change in any release, a patch included.

| Part                                                                                                                                                      | Why it is unstable                                                                                       |
| --------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `world`: `World`, its players, chat, notices, component writes, transactions, captures, kicks, bans and chat mutes                                       | young; world edits and build rights rest on engine facts not yet seen in the running game                |
| `typed`, under its own `typed` feature. That feature turns on `unstable`.                                                                                         | young; its items follow the newest game build until 1.0                                                  |
| `map` and `Load::fog_of_war`. Also the map markers of `ui`.                                                                                                | young; the map frame, the fog and how markers show are not yet seen in the client                        |
| `actions` and `console`                                                                                                                                   | young                                                                                                    |
| `items`. Its `Load::on_item` and `Load::on_item_action`. Also the `on` table of `items.toml`.                                                                   | which input or event each item action raises is known from reflection only, not yet seen in the game     |
| `rewards` and `rewards.toml`                                                                                                                              | young; a reward's experience and a craft's pairing with its request are not yet seen in the game         |
| `script`. `#[derive(Script)]` and `#[script]`.                                                                                                             | the binding of Rust types to Luau, still growing with it                                                 |
| Calls: `#[call]` and `Load::serve_call`. `Load::caller` and `services::Call`. The described calls, events and points a script runtime uses.                   | young                                                                                                    |
| `Effects::teleport` and `Effects::ping`                                                                                                                   | whether a client keeps a teleported player in place, and shows a ping the server raised, is not yet seen |
| `Knowledge::revoke` and its kin, `Load::on_knowledge_input`                                                                                               | how a vanilla client takes a revoke, and what it asks for, is not yet seen                               |
| `Load::request_base_limits` and `altars::set_base_cap`                                                                                                                               | young                                                                                                    |
| `item_data` and `entity_data`. `Stats::set_item`. Rolling affixes onto items with `Affixes::roll`. Also `Load::add_loot_affixes` and `Load::add_craft_affixes`. | after a restart, a value on an item outside a placed container or a player's inventory stays aside        |
| `rates`: every game rate                                                                                                                                  | how a lowered skill-point total and the loot counts play out is not yet seen in the client               |
| Changes of `Field::MaxStackSize` and `Field::Slots` from a mod; the same changes in data packs carry no promise either                                    | how a player's game shows a chest above 72 slots, a raised stack and a grown backpack is not yet seen    |
| Damage filters: `Load::on_damage` and `Damage`. Also `DamageCause` and `events::PlayerDied`. The `killer` fields of `EntityDiedEvent`.                            | the order of the game's attributes and who it records as a killer are not yet seen                       |
| `Load::add_recipe` and `add_recipe_with`. Also `derive_recipes`.                                                                                               | what crafting a new recipe leaves in a player's knowledge is not yet seen                                |
| `input`. `Frame::open_screen` and `Load::on_screen_changed`.                                                                                               | young; which control each bit stands for and which screen is open are not yet seen in the game           |
| `Frame::prompt` and `Frame::screen_host`. Also `interactions::Targets` and `Load::on_fog_uncovered`.                                                        | the prompt, the screen's host, a player's prompt target and the uncovered fog are not yet seen in the game |

The API reference marks each of these items "Unstable". Turn the feature on in
the mod's `Cargo.toml`:

```toml
[dependencies]
enshwick = { git = "https://github.com/enshwick/enshwick", tag = "v1.0.0-alpha.13", features = ["unstable"] }
```

An unstable item leaves the feature once it has settled and, where it rests on
the game, once the running game has shown it. Some chapters use player actions
or the world only to show a stable call in context. Those lines need the
feature; the call itself does not.

## Luau

A Luau mod has no features to turn on. The definitions in `enshwick.d.luau` mark
the unstable parts "Unstable". They are the `world` namespace and
`enshwick.caller`. They
may change in any release, as their Rust counterparts do.

## The ABI

The loader and the mods talk through one C ABI, whatever features a mod turns
on. An unstable call is a slot of the same table as a stable one.

The ABI version is a major and a minor (see
[ABI reference](abi-reference.md#versions)). A loader loads mods built for
its own major, at its own minor or an older one. A release that changes the
layout other than by appending is a new major, and every mod is rebuilt
against it. A release that only adds game
builds keeps the layout and the mods.

From 1.0 the layout is frozen. The loader's table and its structs grow only
by appending fields, and a loader runs mods built against older releases.

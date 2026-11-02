# Stability

From 1.0 the `enshwick` crate follows
[Semantic Versioning](https://semver.org/), so a mod built against 1.x still
builds against every later 1.y and loads in a 1.y loader.

## What is stable

Everything the crate offers without a feature:

- the mod and its loader: `Load`, `Host`, settings, texts, state and migrations;
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
| `world`: `World`, its players, chat, notices, component writes, transactions, captures, kicks, bans and the allowlist                                     | young; world edits and build rights rest on engine facts not yet seen in the running game                |
| `map`, `Load::fog_of_war` and the map markers of `ui`                                                                                                     | young; the map frame, the fog and how markers show are not yet seen in the client                        |
| `actions` and `console`                                                                                                                                   | young                                                                                                    |
| `script`, `#[derive(Script)]` and `#[script]`                                                                                                             | the binding of Rust types to Luau, still growing with it                                                 |
| Calls: `#[call]`, `Load::serve_call`, `Load::caller`, `services::Call` and the described calls, events and points a script runtime uses                   | young                                                                                                    |
| `Effects::teleport` and `Effects::ping`                                                                                                                   | whether a client keeps a teleported player in place, and shows a ping the server raised, is not yet seen |
| `Knowledge::revoke` and its kin, `Load::on_knowledge_input`                                                                                               | how a vanilla client takes a revoke, and what it asks for, is not yet seen                               |
| `Load::request_base_limits`                                                                                                                               | young                                                                                                    |
| `item_data` and `entity_data`, `Stats::set_item`, and rolling affixes onto items: `Affixes::roll`, `Load::add_loot_affixes` and `Load::add_craft_affixes` | after a restart, a value on an item no load or join finds in its slot again stays aside                   |
| `rates`: every game rate                                                                                                                                  | how a lowered skill-point total and the loot counts play out is not yet seen in the client               |
| Changes of `Field::MaxStackSize` and `Field::Slots` from a mod; the same changes in data packs carry no promise either                                    | how a player's game shows a chest above 72 slots, a raised stack and a grown backpack is not yet seen    |
| Damage filters (`Load::on_damage`, `Damage`, `DamageCause`), `events::PlayerDied` and the `killer` fields of `EntityDiedEvent`                            | the order of the game's attributes and who it records as a killer are not yet seen                       |
| `Load::add_recipe`, `add_recipe_with` and `derive_recipes`                                                                                                | what crafting a new recipe leaves in a player's knowledge is not yet seen                                |

The API reference marks each of these items "Unstable". Turn the feature on in
the mod's `Cargo.toml`:

```toml
[dependencies]
enshwick = { version = "1", features = ["unstable"] }
```

An unstable item leaves the feature once it has settled and, where it rests on
the game, once the running game has shown it. Some chapters use player actions
or the world only to show a stable call in context; those lines need the
feature, the call itself does not.

## Luau

A Luau mod has no features to turn on. The definitions in `enshwick.d.luau` mark
the unstable parts "Unstable": the `world` namespace and `enshwick.caller`. They
may change in any release, as their Rust counterparts do.

## The ABI

The loader and the mods talk through one C ABI, whatever features a mod turns
on. An unstable call is a slot of the same table as a stable one.

Until 1.0 the ABI version is a hash of the layout of every ABI type, and a
loader loads only mods built for its own version (see
[ABI reference](abi-reference.md#versions)). A release that changes the
layout needs every mod rebuilt against it, while a release that only adds
game builds keeps the layout and the mods.

From 1.0 the layout is frozen. The loader's table and its structs grow only
by appending fields, and a loader runs mods built against older releases.

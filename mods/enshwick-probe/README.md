# Probe

A development mod, never released: it exercises the platform API on a test
server without a player and logs what it sees to
`logs/enshwick/mods/probe.log`, each check as a `PASS`, `FAIL` or `SKIP`
line. Keep it off servers people play on.

## What it checks

Once the world has loaded, and again on `/probe check`:

- the world key, and that a per-world store opens under it; the store keeps
  a sample of placed entities, and the next check counts how many keep their
  id and template across the save and load in between;
- entity queries: every placed entity, its position against its transform,
  and the entities near it;
- the vitals of actors with replicated health, against that health, and the
  progress of a character;
- component writes: the replicated health is refused as a writable type; an
  actor's health is set one lower, healed by the most negative amount (it
  stays at 1), its replicated copy checked a second later, and put back;
- spawns: a copy of an entity in the world, one with health if there is one,
  three metres above it; that it arrives where asked, its health writes when
  it has health, a teleport five metres away, and its destruction;
- mod messages: that a channel is refused to the probe, which is no join
  mod; messages themselves need a client;
- actions: a ping far from anything, which the game must hand back as a game
  event in the next step; that a type off the allow list and a teleport of
  an entity not in the world are refused;
- that destroy requests refuse entity 0 and an entity not in the world;
- the clock: a reading, a time an hour past the next turn of day or night
  that reads back (the turn's event follows), the time put back, a factor of 10 on the speed that the clock's speed shows, and the
  factor put back; the turns of day and night are logged as they come;
- game settings: a typed read, the perk cost raised by a half that reads
  back with the preset custom, a night of zero refused, and the settings put
  back;
- buffs: the types of the running game; on the first entity that holds
  buffs, its buffs, a shown buff given for 30 seconds that reads back, and
  its removal, gone a second later (skipped without such an entity);
- knowledge: the quests of the running game, a world counter that reads,
  a grant of the wrong kind and one of an unknown id refused, and the
  counter raised by one, read back a second later; knowledge changes and
  completed quests are logged as they come;
- weather: a reading at the first placed entity and the fronts; rain for two
  minutes that shows in the fronts, then clear skies, then the game's own
  choice again; chunk loads and unloads are logged as they come;
- stats: a built-in kind's definition; on the first living entity with
  health, the probe's own kind and 100 extra maximum health set as
  modifiers, both read back a second later (the maximum through the
  entity's vitals), then removed, the maximum back where it was; stat
  changes are logged as they come;
- terrain: the materials, a dry dig at the first placed entity, a mound
  added 40 metres above it and dug away again, and a radius of 100 metres
  refused;
- a save on demand, followed by `WorldSaved`.

From start it counts the damage its damage filter sees by cause, and every
game event by type, and logs the counts every minute. It logs each typed
game event it receives, each fall and shroud damage, and the first 200 hits
a player takes or deals, with the amount before and after the filter; a
typed event whose fields this game build lacks shows as `FAIL typed <type>`.

## Commands

From the chat as an administrator, or through the admin tools' command file
(`mod-data/admin-tools/commands.txt`):

| Command                         | What it does                                                                                                                                           |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/probe census`                 | the game events counted so far, by type                                                                                                                |
| `/probe check`                  | runs the checks again                                                                                                                                  |
| `/probe vitals <id>`            | an entity's vitals and progress, against its replicated health                                                                                         |
| `/probe vitals me [shown]`      | your character's raw health, stamina and mana sets, every slot, the replicated copies and the API's reading, beside the values you read off the screen |
| `/probe teleport me [dx dy dz]` | moves your character by the offset (10 m along x and 0.5 m up by default) and logs where it stands four times a second for five seconds                |
| `/probe destroy <id>`           | asks the world to destroy an entity; on a copy of a world only                                                                                         |
| `/probe spawn <id>`             | spawns a copy of an entity above it and follows it as above                                                                                            |
| `/probe notice <text>`          | shows the text on your screen, or in chat without Enshwick                                                                                             |

## Settings

`mod-configs/probe.toml`, read at start:

```toml
enabled = false
# Refuse every craft request or every placement, to see the vetoes in game;
# the probe logs each craft request and the first placement either way.
refuse_crafting = false
refuse_building = false
# Halve every damage, to see the damage filter in game; the probe counts
# the damage it sees by cause and logs the counts every minute either way.
soften_damage = false
```

Licensed under [MIT](../../LICENSE).

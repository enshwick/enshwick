# Probe

A development mod, never released: it exercises the platform API on a test
server without a player and logs what it sees to
`logs/enshwick/mods/probe.log`, each check as a `PASS`, `FAIL` or `SKIP`
line. Keep it off servers people play on.

## What it checks

Ten seconds after the world has loaded, and again on the `check` action:

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
  factor put back; the turns of day and night are logged as they come; these
  run once the weather checks are done, since moving the clock ends the fronts;
- game settings: a typed read, the perk cost raised by a half that reads
  back with the preset custom, a night of zero refused, and the settings put
  back; then the bosses' health kept at 1.25, read back and forgotten, the
  world's own value back. With `keep_across_restart` the run keeps it for
  the next start instead, and the next run with that setting on sees it come
  back with the world and forgets it: until then the world plays with that
  boss health. Whatever the probe still keeps when the server stops in order
  is forgotten then, unless kept across the restart on purpose; a killed
  server keeps it until the next run;
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
- affixes: the first weapon or armour in any container rolled two affixes
  from a seed and again the same, its lines named, and its old stats put
  back (skipped when no such item is found, or no player was online yet);
- terrain: the materials, a dry dig at the first placed entity, a mound
  added 40 metres above it and dug away again, and a radius of 100 metres
  refused;
- a save on demand, followed by `WorldSaved`.

While enabled, from start it counts the damage its damage filter sees by cause,
and every game event by type, and logs the counts every minute. It logs each
typed game event it receives, each fall and shroud damage, and the first 200
hits a player takes or deals, with the amount before and after the filter; a
typed event whose fields this game build lacks shows as `FAIL typed <type>`.

## Actions

On the mod's page of the Mods screen, for administrators, or through the
admin tools' command file (`mod-data/admin-tools/commands.txt`) as
`probe <action> <values>`, such as `probe vitals 4096`. The console has no
character, so the actions on your own character work from the Mods screen
only.

| Action      | What it does                                                                                                |
| ----------- | ----------------------------------------------------------------------------------------------------------- |
| `census`    | the game events counted so far, by type                                                                     |
| `check`     | runs the checks again                                                                                       |
| `key`       | your account key                                                                                            |
| `vitals`    | an entity's vitals and progress, against its replicated health                                              |
| `my-vitals` | your character's raw health, stamina and mana sets, every slot, the replicated copies and the API's reading |
| `teleport`  | moves your character by the offset given and logs where it stands four times a second for five seconds      |
| `destroy`   | asks the world to destroy an entity; on a copy of a world only                                              |
| `spawn`     | spawns a copy of an entity above it and follows it as above                                                 |
| `notice`    | shows a notice on your screen, or in chat without Enshwick                                                  |
| `knowledge` | a knowledge value of yours, or of the world for its ids                                                     |
| `learn`     | grants it to you: a bool set, a counter raised by the bit or amount, that flag set                          |
| `unlearn`   | takes it back the same way                                                                                  |
| `stat`      | adds a value to one of your stats                                                                           |
| `stat-off`  | removes what the probe added to that stat                                                                   |
| `stats`     | your stats that differ from the defaults, and their sources                                                 |
| `affix`     | rolls one to three affixes anew on everything you wear                                                      |

## Settings

`mod-configs/probe.toml`, read as the game starts, for every world; a change
takes effect when the game starts again:

```toml
enabled = false
# Refuse every craft request or every placement, to see the vetoes in game;
# the probe logs each craft request and the first placement either way.
refuse_crafting = false
refuse_building = false
# Halve every damage, to see the damage filter in game; the probe counts
# the damage it sees by cause and logs the counts every minute either way.
soften_damage = false
# Keep the bosses' health at 1.25 for the next start, to see a kept game
# setting come back with the world; the next run with this on forgets it.
keep_across_restart = false
```

`enabled` is off by default: until it is `true` the probe does nothing, no
checks, counts or actions, and the other settings have no effect.

Licensed under [MIT](../../LICENSE).

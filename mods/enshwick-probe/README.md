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
- a save on demand, followed by `WorldSaved`.

From start it counts every game event by type and logs the counts every
minute, and logs each typed game event it receives; a typed event whose
fields this game build lacks shows as `FAIL typed <type>`.

## Commands

From the chat as an administrator, or through the admin tools' command file
(`mod-data/admin-tools/commands.txt`):

| Command               | What it does                                                   |
| --------------------- | -------------------------------------------------------------- |
| `/probe census`       | the game events counted so far, by type                        |
| `/probe check`        | runs the checks again                                          |
| `/probe vitals <id>`  | an entity's vitals and progress, against its replicated health |
| `/probe destroy <id>` | asks the world to destroy an entity; on a copy of a world only |
| `/probe spawn <id>`   | spawns a copy of an entity above it and follows it as above    |
| `/probe notice <text>`| shows the text on your screen, or in chat without Enshwick     |

## Settings

`mod-configs/probe.toml`, read at start:

```toml
enabled = false
# Refuse every craft request or every placement, to see the vetoes in game;
# the probe logs each craft request and the first placement either way.
refuse_crafting = false
refuse_building = false
```

Licensed under [MIT](../../LICENSE).

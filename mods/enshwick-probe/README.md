# Probe

A development mod, never released: it exercises the platform API on a test
server without a player and logs what it sees to
`logs/enshwick/mods/probe.log`, each check as a `PASS`, `FAIL` or `SKIP`
line. Keep it off servers people play on.

## What it checks

Once the world has loaded, and again on `/probe check`:

- the world key, and that a per-world store opens under it;
- entity queries: every placed entity, its position against its transform,
  and the entities near it;
- the vitals of actors with replicated health, against that health, and the
  progress of a character;
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

## Settings

`mod-configs/probe.toml`, read at start:

```toml
enabled = false
```

Licensed under [MIT](../../LICENSE).

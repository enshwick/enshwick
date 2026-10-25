# Research

A development mod, never released: it logs what a running client does where
the platform still needs engine facts, to `logs/enshwick/mods/research.log`.
It only reads; the first bytes of each function it hooks or calls are
checked against the client build its address was found on (1269579), and
what depends on a function that differs stays off. Keep it off games people
play.

## What it logs

- The item tooltip: whenever what they are handed changes, at most once a
  second each, the arguments of the tooltip's entry and body, of the item
  viewers that build its stat rows and of the function that draws the rows.
  The tooltip's description is shown with what its pointers point at. Lines
  start with `R1 dump`; each names the function and who called it.
- The interface, whenever what a call sees changes: each scroll area's start
  (who opens it and its kept state) and its offset as the wheel, the
  scrollbar or the gamepad moves it (`R2 scroll begin`, `R2 scroll end`);
  each tab bar's labels and chosen tab, the world settings' among them
  (`R3 tabs`); the game's text input dialog when it opens (`R5 text dialog`)
  and each text field's text as it is typed (`R5 text field`).
- Join attempts: each join the game starts, with its host id, address and
  ports and how long the call took, and for 30 seconds after, once a second,
  how many main- and pause-menu entries were drawn. Lines start with `R8`.
- Entity ids, on the client and on the server: every five seconds, the entities
  within 20 m of each player, at most 40 a sweep, with their id, template GUID
  and position, and the entity ids that `DespawnLink`, `OwnerRelationship`,
  `NetworkComfort` and `AggroTargetNetworkData` hold with what those ids name on
  the same side. The client adds each entity's slot and record. Joining a
  client's and a server's log by template and position shows which ids a client
  gives the server's entities. Lines start with `R11 client` or `R11 server`; a
  server without players sweeps once around its first entity.
- Health set by the game's impact programs, outside hits: heals, revives,
  scripted kills. For the first 300 of a run, on the client and on the
  server, the entity, the value's type and the value set. Lines start with
  `R17 health write`.
- Fibers: every ten seconds for five minutes after the start, each other
  thread is stopped for a moment; threads whose stack pointer lies outside
  the stack their thread block records, or that were made fibers, are
  listed in one `R18` line at the end.
- Skills: the first 500 unlocks and refunds of skill nodes in a run, each with
  the node, whether a level was added or taken, the answer and the node's level
  before and after. Lines start with `R19 skill change`.
- Spawn targets, on the server: every five seconds, each player's spawn
  target (slot 9 of `SelectedTargets`: an entity and a place) when it
  changes, with where the player stands. Lines start with `R20`.

## Commands

From the chat as an administrator, on a dedicated server or while the client
hosts a game:

| Command                  | What it does                                                                                                                       |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `/research mark [words]` | a marked line in the log; every tooltip function dumps its next call, and numbers among the words are looked for in what is dumped |
| `/research status`       | how many tooltip dumps were written since the last mark, and how many impact writes were seen                                      |

## Settings

`mod-configs/research.toml`, read at start:

```toml
enabled = false
# Look at the item tooltip, the interface and join attempts, and log the
# entities near each player.
tooltip = true
ui = true
join = true
entities = true
# Log the health impact programs set, and look for threads on fibers.
impact_health = true
fibers = true
# Log skill unlocks and refunds, and each player's spawn target.
skills = true
respawn = true
# The least time between two dumps of one tooltip function, unless a mark
# comes between, and the most dumps between two marks.
dump_interval_ms = 1000
max_dumps = 300
```

Licensed under [MIT](../../LICENSE).

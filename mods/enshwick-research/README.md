# Research

A development mod, never released: it logs what a running client does where
the platform still needs engine facts, to `logs/enshwick/mods/research.log`.
It only reads; each hooked function's first bytes are checked against the
client build its address was found on (1269579), and a function that
differs stays unhooked. Keep it off games people play.

## What it logs

- The item tooltip: whenever what they are handed changes, at most once a
  second each, the arguments of the tooltip's entry and body, of the item
  viewers that build its stat rows and of the function that draws the rows.
  The tooltip's description is shown with what its pointers point at. Lines
  start with `R1 dump`; each names the function and who called it.
- Join attempts: each server change the game asks for, what the call
  answers (0 when the join check refused it), and for 30 seconds after, once
  a second, how many main- and pause-menu entries were drawn. Lines start
  with `R8`.

## Commands

From the chat as an administrator while the client hosts a game:

| Command                  | What it does                                                                                                                       |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `/research mark [words]` | a marked line in the log; every tooltip function dumps its next call, and numbers among the words are looked for in what is dumped |
| `/research status`       | how many tooltip dumps were written since the last mark                                                                            |

## Settings

`mod-configs/research.toml`, read at start:

```toml
enabled = false
# Look at the item tooltip and at join attempts.
tooltip = true
join = true
# The least time between two dumps of one tooltip function, unless a mark
# comes between, and the most dumps between two marks.
dump_interval_ms = 1000
max_dumps = 300
```

Licensed under [MIT](../../LICENSE).

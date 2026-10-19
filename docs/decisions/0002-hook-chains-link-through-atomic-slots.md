# 0002: Hook chains link through atomic slots

Accepted, 2026-10-20.

In the context of several mods hooking the same game functions, some called many
times per frame, facing the cost of every instruction on those paths and the
need to drop a faulty mod while the game runs, we decided for one detour per
target with each mod's link calling the next through a static atomic slot the
loader points, and against an event bus or repatching code per mod, to achieve
one load and one indirect call per link and fault isolation without touching
code, accepting that links run in a fixed order set at arming.

## Context

The loader detours a game function once. Lookups such as the survivor map are
called from many places every frame, so dispatch overhead matters. A mod that
panics must stop acting without crashing the game and without the loader
rewriting machine code while game threads run it.

## Decision

Each link is a `Hook` with a static `AtomicUsize` that the loader sets to the
next link or the original at arming; the link calls through it with `Next::get`,
and the loader removes a faulty mod by repointing the slot before its links.

## Considered options

| Option                                   | Good                                        | Bad                                                                |
| ---------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------ |
| Atomic slot per link                     | one load and one call; unlinking is a store | order fixed at arming                                              |
| Event bus with priorities and cancelling | flexible, familiar from Forge               | an event object per call on per-frame paths                        |
| A detour per mod, patched in turn        | no loader logic                             | unhooking means repatching live code; order depends on load timing |

## Consequences

Per-call cost stays close to a plain detour. Rare events still get a bus, so the
two tiers do not mix. A link that needs the game's call site asks the loader for
the caller frame instead of reading its own stack.

## Confidence

High for hot paths. Measured overhead above a few nanoseconds per link would
reopen it.

## Compliance

The lifecycle test checks chaining and the fall-through on panic; the loader's
benchmarks measure dispatch.

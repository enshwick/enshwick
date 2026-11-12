<!--
type: explanation
description: What Enshwick costs at run time: native code with no garbage collector for Rust, C and C++ mods, an interpreter for Luau, and the costs measured.
-->

# Performance

Rust, C and C++ mods compile to native code, as the game does. Luau scripts
run in an interpreter. For code that runs on every hit or frame, write a native
mod.

## Why do native mods cost little?

The loader and every Rust, C or C++ mod run as machine code inside the game's
process. No managed runtime starts with them, and no garbage collector pauses
the game for them.

A hook reaches its mod through a plain function call. Calling the next hook
through `Next` costs one atomic load and one indirect call
([Hooks](../hooks.md)). An event that no mod subscribed to costs nothing
([Events and the scheduler](../events.md)).

The loader's code that runs on each call or frame is written not to allocate,
lock or log.

## What does a Luau script cost?

A Luau mod runs in a virtual machine of its own, through Luau's interpreter.
An interpreter does more work per call than native code, so a script runs
slower than the same logic in a native mod.

A handler that runs once per event or step costs little either way. A function
that runs on every hit or every frame is best written in Rust, C or C++. The
budgets each Luau mod has are in [Luau mods](../luau.md#the-sandbox).

## Which costs are measured?

Each row is one kind of call, measured alone. No row is a cost for a whole
game, and the rows come from different setups, so do not compare them.

| What | Cost | Measured how |
| --- | --- | --- |
| A mid-function hook, per pass, with an empty handler | about 5 to 22 ns, by the code path the processor takes; one handler adds about 4 ns | a release test on an AMD Ryzen 9 9950X3D that runs every path ([Mid-function hooks](../mid-function-hooks.md)) |
| One event handler call | about 1 µs, mostly the processor fetching the mod's code and data | a server copy of game build 399 with 11 mods ([Events and the scheduler](../events.md)) |
| One Luau damage filter call | about 1.15 µs, 0 allocations in 10,000 warm calls | `cargo bench -p enshwick-luau --bench calls` on the fake host, release build, AMD Ryzen 9 9950X3D, 2026-11-10 |
| A Luau call into the API with number arguments | about 0.13 µs, no allocation | the same bench, 2026-11-13, while other builds ran on the machine |
| A Luau call into the API with a string | about 0.24 µs, 1 allocation | the same bench and day |
| A Luau call into the API with a buffer | about 0.2 µs, 1 allocation | the same bench and day |
| A Luau call into the API with a list | about 0.4 µs, 2 allocations | the same bench and day |
| A Luau call into the API with a function | about 0.6 µs, 1 allocation | the same bench and day |
| A Luau call into the API with a record of two numbers and a [content id](ids.md) | about 1.1 µs, 3 allocations | the same bench and day |
| A log line above the mod's level | one atomic load | [Logging and diagnostics](../logging.md) |
| Timing a callback, with `diagnostics = true` | about 14 ns | [Logging and diagnostics](../logging.md#per-mod-costs) |

## How do I see what my mod costs?

Set `diagnostics = true` in `enshwick.toml` in the game folder. The status
summary then lists each mod's events and filters with their time, and its hook
calls. Add `diagnostics_timing = true` to time hook links too. See
[Per-mod costs](../logging.md#per-mod-costs).

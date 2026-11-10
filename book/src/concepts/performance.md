<!--
type: explanation
description: What Enshwick costs at run time: native code with no garbage collector for Rust, C and C++ mods, an interpreter for Luau, and the costs measured.
-->

# Performance

Rust, C and C++ mods compile to native code, as the game does. Luau scripts
run in an interpreter and cost more per call. This page gives the costs
measured so far.

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

A Luau mod runs in a virtual machine of its own, through Luau's interpreter. A
call into a script costs more than a call into native code. One damage filter
call costs about 1.15 µs, and a warm one allocates nothing.

A handler that runs once per event or step costs little either way. A function
that runs on every hit or every frame is best written in Rust, C or C++. The
budgets each Luau mod has are in [Luau mods](../luau.md#the-sandbox).

## Which costs are measured?

These are costs of single calls, not of a whole game.

| What | Cost | Source |
| --- | --- | --- |
| A mid-function hook, per pass, with an empty handler | about 5 to 9 ns, by processor | [Mid-function hooks](../mid-function-hooks.md) |
| One event handler call on a server | about 1 µs, mostly the processor fetching the mod's code and data | [Events and the scheduler](../events.md), a server copy with 11 mods |
| One Luau damage filter call | about 1.15 µs, 0 allocations in 10,000 calls | `cargo bench -p enshwick-luau --bench damage_filter`, release build, AMD Ryzen 9 9950X3D, 2026-11-10 |
| A log line below the mod's level | one atomic load | [Logging and diagnostics](../logging.md) |
| Timing a callback, with `diagnostics = true` | about 14 ns | [Logging and diagnostics](../logging.md#per-mod-costs) |

## How do I see what my mod costs?

Set `diagnostics = true` in `enshwick.toml` in the game folder. The status
summary then lists the time each mod's hooks, events and filters took. See
[Per-mod costs](../logging.md#per-mod-costs).

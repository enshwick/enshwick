# Luau mods

A Luau mod is a [data pack](content/packs.md) with scripts: a folder in
`mods/` with a `pack.toml`, a `main.luau`, and any other `.luau` files it
loads, up to four folders deep.

## What a Luau mod can do

Luau support is in progress. Today a Luau mod hears events, works with
players, chat, timers, inventories and its settings, and talks to other mods.
It cannot set game rules yet, such as rates or limits. A
[Rust mod](getting-started.md) can.

## How the loader runs it

The loader runs it as it runs any pack: it is in the mod list and the
lock, counts in [the join check](concepts/join-check.md) with the hash of
every script, has its log `logs/enshwick/mods/<id>.log`, and is turned off
alone when it goes wrong. It may hold data files too, such as `items.toml`.

```text
mods/
  you.greeter/
    pack.toml
    main.luau
    lib/
      greet.luau
    types/
      enshwick.d.luau
```

Scripts are [Luau](https://luau.org), Roblox's typed dialect of Lua 5.1.
Each mod runs in a sandbox of its own and reaches the game only through the
global `enshwick`.

## Start a Luau mod

1. Run `cargo enshwick new --luau you.greeter`. It makes the folder with
   `pack.toml`, a `main.luau` that greets each player who joins, a README,
   and `types/enshwick.d.luau`, the API's definitions for the editor.
   `--side` sets where it runs, the server when left out. With
   `--game <game folder>` it also writes `types/game.d.luau`, the
   [game's own types](game-types.md), such as the fields
   `enshwick.on_game_type` hands a handler.
2. Open the folder in an editor with the
   [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp) extension. The
   folder's `.vscode/settings.json` points it at the definitions, so every
   call and event shows its types and documentation.
3. Check the types without the game:

   ```text
   luau-lsp analyze --definitions:@enshwick=types/enshwick.d.luau main.luau
   ```

   With the game's types, add `--definitions:@game=types/game.d.luau`.
4. Copy the folder into the game's `mods/` folder as `mods/you.greeter/`.

The repository's
[`examples/you.welcome`](https://github.com/enshwick/enshwick/tree/main/examples/you.welcome)
is a Luau mod to start from.
`cargo enshwick luau --out types/enshwick.d.luau` writes the definitions
again for the loader the tool was built with, after an update, and
`cargo enshwick types --from <game folder> --luau types/game.d.luau` the
game's types after a game update.

## What a script reaches

```luau
--!strict
enshwick.on("PlayerJoined", function(player)
    enshwick.world.broadcast(`entity {player.entity} joined`)
end)

enshwick.scheduler.every(60, function()
    enshwick.log.info(`{#enshwick.world.players()} players online`)
    return true
end)
```

| Name | What |
| --- | --- |
| `enshwick.on(event, handler)` | calls `handler` on each loader [event](events.md) or typed [game event](#game-events), with its data as a table; only at the top level of `main.luau` |
| `enshwick.on_game_type(type, fields, handler)` | a game event by its reflected type, with the fields named |
| `enshwick.extend(point, handler)` | a link into another mod's [extension point](#extending-another-mod) |
| `enshwick.on_mod_event(event, handler)`, `enshwick.define_event(event)` | [mod events](#mod-events), heard and sent |
| `enshwick.caller(call)` | a function that runs [another mod's call](#calling-another-mod); unstable |
| `enshwick.deps` | the mods `pack.toml` names in `requires`, `after` and `recommends`, [by alias](#other-mods-by-alias) |
| `enshwick.log` | `info`, `warn` and `error` write a line to the mod's log; `print` writes as `info` |
| `enshwick.world` | the connected players, chat, notices and positions of [the world](world.md); unstable |
| `enshwick.inventory` | slots and counts of [inventories](inventories.md), and items given, taken and moved, on the server |
| `enshwick.settings()` | the mod's [settings](#settings) in force, as a table |
| `enshwick.scheduler` | `next_tick`, `after` and `every` run a function on the game thread; an `every` task stops when it returns `false` |
| `require("./name")` | another script of the same folder, as Luau resolves it |

The rows marked unstable may change in any release, as the Rust parts behind
them do ([Stability](stability.md)); a script uses them with no switch.

Names are the Rust API's: `enshwick.world.notify_all` is
`World::notify_all`, and the event `PlayerJoined` is `events::PlayerJoined`.
The definitions file lists every call with its documentation. Values take
these Luau types:

| Rust | Luau |
| --- | --- |
| numbers up to 32 bits, `f32`, `f64`, entity ids | `number` |
| `u64`, such as an account key | `string` of its decimal digits |
| `&str`, `String`, a GUID | `string` |
| a position, `[f64; 3]` | `vector` |
| `Option<T>` | `T?` |
| `Vec<T>` | `{T}` |
| a struct | a table with its fields |
| an enum such as `Notice` | the variant's name, `"OnScreen"` |
| `Duration` | seconds |

The Luau API is not a second copy: it is read from descriptions kept beside
the Rust items in `enshwick::script`. `#[derive(Script)]` describes a type,
`#[script(<part>)]` the methods of an `impl` block a script may call, and the
event tables describe each event, so a new event or a newly marked method
reaches Luau and the definitions with no other change.

A call that fails raises a Luau error with the call's name and the
reason, such as `enshwick.world.broadcast: text: expected a string, got a
number`. `pcall` catches it.

## Inventories

`enshwick.inventory` reads and changes inventories on the server, as
[`Inventories`](inventories.md) does in Rust. `slots` and `count` read at
once; `give`, `take`, `destroy_slot`, `move_slot` and `move_all` run in the
game's next inventory step and hand the outcome to a function:

```luau
local GIFT = 3073 -- an item id from the game's data

enshwick.on("PlayerJoined", function(player)
    local held = enshwick.inventory.count(player.entity, GIFT)
    enshwick.inventory.give(player.entity, GIFT, 10, function(outcome)
        if outcome.done ~= "Committed" then
            enshwick.log.warn(`no gift for {player.entity}: {outcome.done}`)
        end
    end)
    enshwick.log.info(`{player.entity} held {held} before the gift`)
end)
```

`outcome.done` is `Committed`, `RolledBack`, `Unavailable`, `TimedOut` or
`Refused`, and each entry of `outcome.ops` has the operation's `error` by
name and the count it fell short by. A slot is a `string` key, as `slots`
returns it in `at`.

## Game events

`enshwick.on` takes the name of a typed [game event](game-events.md) as
well, such as `HitEvent` or `EntityDiedEvent`. The handler gets the event's
fields as `game_events::HitEvent` has them, a mask as the list of its bit
names, a game value as its name. A value the API does not name arrives as
`Other(<number>)`, a name of ours with the game's number in it:

```luau
enshwick.on("HitEvent", function(hit)
    if hit.health_change < 0 and hit.root_source_id then
        enshwick.log.info(`{hit.root_source_id} hit {hit.target_id}`)
    end
end)
```

Any other game event is read by its reflected type and the fields the script
names, each with how to read it: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`,
`u64`, `i64`, `f32`, `f64`, `bool` or `entity`, the 64-bit ones as strings.
`enshwick.game_event_types()` lists every type of the running build with
its size. A field the type lacks on this build leaves the mod off at start,
with the reason in the log.

```luau
enshwick.on_game_type("keen::ecs::EntityDiedEvent", { deadEntityId = "entity" }, function(event)
    enshwick.log.info(`entity {event.deadEntityId} died`)
end)
```

Game events run on the server's game thread before the step's systems, as
for a Rust mod; on a client, only while it hosts a game.

## Other mods by alias

`pack.toml` names the mods a Luau mod depends on under aliases, as a
[data pack](content/packs.md#packtoml) does. `enshwick.deps` holds a table
per alias of `requires`, `after` and `recommends`, as `deps!` does for a Rust
mod: `id` is the mod's id, and `running()` says whether it runs now. A mod of
`requires` always runs beside this one. Read `running()` for one of `after`
or `recommends`:

```toml
[recommends]
lanterns = "you.lanterns"
```

```luau
if enshwick.deps.lanterns.running() then
    enshwick.log.info(`{enshwick.deps.lanterns.id} lights the way`)
end
```

## Extending another mod

A Luau mod links into another mod's [extension point](extending.md) as a
Rust mod does. `cargo enshwick deps <alias> --from <owner's DLL> --luau`
writes a module for each point, call and event of the dependency that
`pack.toml` names `<alias>` in `[requires]`, `[recommends]` or `[after]`, as
`deps/<alias>/<name>.luau` with the owner's local name. A point's module holds
the types it reaches, its named values, and each field's place with the layout hash the
owner's build wrote. The loader links only when every hash matches the
owner's, the input the module reads is no larger than the owner's and the
answer has the owner's size. A script changes an answer only of a number,
`boolean`, GUID or id type (`Entity`, `ItemId`, `RecipeId`, `KnowledgeId`,
`SkillNode`, `PlayerKey`, `BuffTypeId`, `NpcId`), whose hash the runtime checks against the
type the module names; on another answer type it may observe the point. The handler gets the input and the answer so
far, and returns a new answer, `nil` to keep it, or `enshwick.refuse` on a
refusable point:

```luau
local price = require("./deps/trading_post/price")

enshwick.extend(price, function(input: price.Price, answer: number)
    if input.deal == price.Deal.SELL then
        return answer * 2
    end
    return nil
end)
```

The handler runs on the thread that asks the point, under the same budget
as an event handler. An error or an answer of the wrong type turns the mod
off, and the answer stays as it was. An owner version with another layout
leaves the link idle, with the reason in the log, until the module is
written again.

## Mod events

A Luau mod hears another mod's [events](services.md) and sends its own.
`cargo enshwick deps <alias> --luau` writes the description of each event the
owner declares, beside its points. A description may also be written by hand:
each field with a `name` and how it reads, `u8` to `i64`, `f32`, `f64`,
`bool`, `guid`, or an id: `entity`, `item_id`, `recipe_id`,
`knowledge_id`, `skill_node`, `player_key`, `buff_type_id` or `npc_id`, laid out and hashed as Rust lays out a
`#[repr(C)]` struct of those fields:

```luau
local greeted = enshwick.define_event({ name = "greeted", fields = {
    { name = "player", read = "entity" },
    { name = "times", read = "u16" },
} })

enshwick.on_mod_event(require("./deps/trading_post/sold"), function(sold)
    greeted.send({ player = sold.character, times = 1 })
end)
```

The loader matches a subscription by its layout hashes, as for a Rust mod,
and leaves it idle, with the reason in the log, when the defining mod's
event differs or is smaller. `define_event` takes the event's local name:
`greeted` is `you.greeter:greeted`, as `#[event(name = "greeted")]` is for a
Rust mod. An event a script defines has primitive fields only. `define_event` raises an error for a new event, or for changed
fields, after the start: either takes the next start. A `send` before
`main.luau` has finished reaches no one. Other mods hear the event only when
`pack.toml` exports it, as `exports = ["greeted"]`.

## Calling another mod

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

A Luau mod calls another mod's [call](calls.md) through the module
`cargo enshwick deps <alias> --luau` writes for it, as for a point:

```bash
cargo enshwick deps trading_post --from <server DLL> --luau
```

`enshwick.caller` at the top level of `main.luau` returns the function that
runs it once the start is over:

```luau
local price_of = enshwick.caller(require("./deps/trading_post/price-of"))

enshwick.on("WorldSaved", function()
    local coins, why = price_of({ item = 7, count = 1 })
    if coins then
        enshwick.log.info(`item 7 sells for {coins}`)
    else
        enshwick.log.warn(`no price: {why}`)
    end
end)
```

The function answers nil and why not: `unmatched` when no running mod serves the
call as described or the start is not over, `off` when the server or its handler
is, and `failed` when its handler failed. A call asked for after the start
raises an error, and so does a reload that describes an asked call differently:
either change takes the next start.

The input's fields are numbers, booleans, entities and GUIDs. The runtime works
out each field's layout hash from its name, type and offset and refuses a
module whose hashes do not match, so a module edited by hand no longer reaches
the server. A call whose input holds a value type or an enum is for Rust mods
only: `cargo enshwick deps --luau` leaves it out and says why, since a script could forge such a
field's description and send the server a value its type forbids. The answer
may be of any type.

## Settings

A Luau mod declares its settings in `pack.toml`, one `[[setting]]` table
each, in the order the Mods screen shows them:

```toml
[[setting]]
key = "greeting"
default = "Welcome"
doc = "What a player who joins reads before their name."

[[setting]]
key = "announce_minutes"
default = 30
min = 1
max = 1440
stage = "restart"
doc = "How often the log counts the players online, in minutes."
```

| Field | Meaning |
| --- | --- |
| `key` | the key in `mod-configs/<id>.toml`, of `[a-z0-9_]` |
| `default` | the value on first start; its type is the setting's |
| `doc` | what it does, for the file's comments and the Mods screen |
| `stage` | when a change takes effect, as for a Rust mod's [settings](settings.md): `runtime` (the default), `startup`, `restart`, `client` or `sync` |
| `synced` | with `stage = "startup"`, a joined player takes the server's value |
| `min`, `max` | the range of a number |
| `choices` | the values it accepts |
| `text` | the key of its label in the pack's `texts.toml`; `config.<key>` unless given |

The loader writes the file from these on first start, checks every value
against its declaration, and shows the settings behind the mod's gear on the
Mods screen, as it does a Rust mod's. `enshwick.settings()` returns the
values in force as a table. A `SettingsChanged` handler runs after the new
values are in place, so it reads them again there:

```luau
enshwick.on("SettingsChanged", function()
    enshwick.log.info(`greeting now {enshwick.settings().greeting}`)
end)
```

## The sandbox

A script reaches the game only through `enshwick`. It has Luau's own
libraries for tables, strings, `utf8`, `bit32`, `math`, `buffer`, `vector`,
coroutines and the clock (`os.clock`, `os.time`, `os.date`), and nothing
that reads files, starts processes or loads compiled code. The globals and
the `enshwick` tables are read-only. `require` resolves only the mod's own
scripts.

Each mod has these budgets:

| Budget | Limit | Past it |
| --- | --- | --- |
| memory | 64 MiB for the mod's whole VM | the allocation fails |
| time per call | 10 ms per call into the scripts, 2 s for `main.luau` at start | the script stops; `pcall` cannot catch it |
| time per second | 100 ms for all calls into the scripts within one second | the mod turns off |

A server step takes about 50 ms, so a handler that runs on every step does
little work each time and leaves the rest to a task.

## When a script fails

An error that leaves a handler or a task, a script past a budget, or a panic
in a call it made turns that mod off, as a panic turns off a Rust mod. The
log names the file and line:

```text
WorldSaved: main.luau:2: boom
```

The game and every other mod run on. A `main.luau` that does not compile or
fails at start leaves the mod off from the start, with the same line.

## Threads

`main.luau` runs on the loader's start thread, and after a reload on the
thread that watches files. Event handlers and tasks run
on the server's game thread, between simulation steps, where calls into the
world are allowed (see [Threads](concepts/threads.md)). Hooks and filters
inside a step stay in Rust: a Luau mod sees their results through events.

## Change scripts while the game runs

With `dev_reload = true` in the loader's settings, the loader looks at a Luau
mod's folder every second: a script added, removed, renamed or changed, in
any subfolder the mod may hold scripts in, counts as a change. A new mod
folder waits for the next start. On a change the loader runs `main.luau`
again in a new VM: handlers of
events subscribed at start move to the new scripts, and tasks of the old ones
stop. A handler of an event the old scripts did not subscribe runs from the
next start, and the log says so. When the new scripts fail, the old ones keep
running. Without `dev_reload`, a script change waits for the next start. The
pack's data files reload as any [data pack](content/packs.md)'s do.

## What a mod costs

With `diagnostics = true`, the status summary shows each Luau mod's memory,
its peak, its calls and how many ran past their budget, beside the time its
handlers took (see [Logging and diagnostics](logging.md)).

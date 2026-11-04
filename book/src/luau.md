# Luau mods

A Luau mod is a [data pack](content/packs.md) with scripts: a folder in
`mods/` with a `pack.toml`, a `main.luau`, and any other `.luau` files it
loads, up to four folders deep.

## What a Luau mod can do

A Luau mod reaches the API a Rust mod has, through the global `enshwick`:
each module of the Rust API is a table, `enshwick.<module>.<function>`, with
the Rust names. Each chapter of this book shows the Luau beside the Rust.
Where Luau differs, [the table below](#where-luau-differs-from-rust) says
why.

## How the loader runs it

The loader runs it as it runs any pack: it is in the mod list and the
lock, counts in [the join check](concepts/join-check.md) with the hash of
every script, has its log `logs/<id>_<date>_<time>.log`, and is turned off
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

## How the Rust API reads in Luau

```luau
--!strict
enshwick.on("PlayerJoined", function(player)
    enshwick.world.broadcast(`entity {player.entity} joined`)
end)

enshwick.schedule.every(60, function()
    enshwick.log.info(`{#enshwick.world.players()} players online`)
    return true
end)
```

Names are the Rust API's: `enshwick.world.notify_all` is
`World::notify_all`, and the event `PlayerJoined` is `events::PlayerJoined`.

- A handle a Rust mod gets from `Load`, such as `load.weather()`, is its
  module's table: `enshwick.weather.at(position)` is `weather.at(position)`.
  On a game build without what the handle needs, the table keeps only its
  start-time functions, and the log says why, so a script checks the
  function it calls: `if enshwick.weather.at then`.
- A handle whose opening switches a loader feature on opens only when the
  script asks, as a Rust mod asks `Load` in `init`: `enshwick.altars.altars()`,
  `enshwick.stats.stats()` and `enshwick.map.fog_of_war()` at the top level
  of `main.luau` open it and return it as an object. Until then the table's
  functions raise an error, so a mod that never asks arms no altar hooks and
  copies no fog.
- A call a Rust mod makes on `Load` during `init`, such as
  `load.define_affix(...)`, is a function of its module's table that a script
  calls at the top level of `main.luau`: `enshwick.affixes.define_affix(...)`.
  After the start it raises an error.
- A Rust handle a call returns, such as a `ClockSpeed` or a `Texts`, is a
  Luau object with the handle's methods: `speed:set(2)`. A method that takes
  `self` by value returns the next object, as a builder does.
- A hook or filter takes a function: it gets the value Rust hands a
  closure, and returns the changed value or nothing to keep it.

These belong to the `enshwick` table itself:

| Name | What |
| --- | --- |
| `enshwick.on(event, handler)` | calls `handler` on each loader [event](events.md) or typed [game event](game-events.md#from-luau), with its data as a table; only at the top level of `main.luau` |
| `enshwick.on_game_type(type, fields, handler)` | a game event by its reflected type, with the fields named |
| `enshwick.extend(point, handler, order)` | a link into another mod's [extension point](extending.md#from-luau) |
| `enshwick.on_mod_event(event, handler)`, `enshwick.define_event(event)` | [mod events](services.md#mod-events), heard and sent |
| `enshwick.caller(call)` | a function that runs [another mod's call](calls.md#from-luau); unstable |
| `enshwick.deps` | the mods `pack.toml` names in `requires` and `after`, [by alias](services.md#other-mods-by-alias) |
| `enshwick.settings()` | the mod's [settings](settings.md#settings-of-a-luau-mod) in force, as a table |
| `require("./name")` | another script of the same folder, as Luau resolves it |

Calls whose Rust parts are unstable may change in any release, as the Rust
parts do ([Stability](stability.md)); a script uses them with no switch.
The definitions file lists every call with its documentation. Values take
these Luau types:

| Rust | Luau |
| --- | --- |
| numbers up to 32 bits, `f32`, `f64`, entity ids | `number` |
| `u64`, such as an account key | `string` of its decimal digits |
| `&str`, `String`, a GUID, a content id, a path | `string` |
| a position, `[f64; 3]` | `vector` |
| bytes, `&[u8]` | `buffer` |
| `Option<T>` | `T?` |
| `Vec<T>` | `{T}` |
| a pair, such as `(String, String)` | a list of the two, `{ a, b }` |
| a struct | a table with its fields |
| an enum such as `Notice` | the variant's name, `"OnScreen"`, or `{ Name = value }` for a variant with a value |
| a Rust handle | an object with its methods |
| `Duration` | seconds |

The Luau API is not a second copy: it is read from descriptions kept beside
the Rust items in `enshwick::script`. `#[derive(Script)]` describes a type,
`#[script(<part>)]` the methods of an `impl` block a script may call, and the
event tables describe each event, so a new event or a newly marked method
reaches Luau and the definitions with no other change.

A call that fails raises a Luau error with the call's name and the
reason, such as `enshwick.world.broadcast: text: expected a string, got a
number`. `pcall` catches it.

## Where Luau differs from Rust

Every public function of the Rust API has a Luau function of the same
module and name, `enshwick.<module>.<name>`, or a Rust handle's method on the
Luau object, `handle:<name>()`; a value, such as an item or a change, is a
table with the struct's fields. A test of the repository fails for any
function without one, unless this table names it and says why.

| Rust | Why |
| --- | --- |
| `abi::*` | The binary interface between the loader and a mod's DLL. A Luau mod has no DLL: its runtime crosses the interface for it. |
| `raw::*`, `code::*`, `Hook::*`, `Frame::*`, `Load::hook`, `Load::hook_at`, `Load::code`, `Load::find_code`, `Load::patch_code`, `Load::symbol`, `Load::constant`, `Load::field_offset`, `Load::type_size`, `Load::system` | Native code: detours, code patches, byte patterns and raw addresses run machine code in the game's own frames. A script runs in a sandbox without pointers. |
| `Host::api`, `Host::handle`, `Host::caller_frame`, `Host::fault`, `host` | The raw handle to the loader. The runtime holds it for the script. |
| `script::*` | The script binding itself, which the Luau runtime runs on. |
| `test::*` | The fake loader a Rust mod's tests run against. A Luau mod is checked with `luau-lsp analyze` and on a server. |
| `shape::*` | Layout hashes of Rust types. The runtime hashes a script's descriptions itself. |
| `Off::new`, `Off::text` | Luau reports a failure with `error()`, and a caller catches it with `pcall`. |
| `content::Blob::*`, `content::ItemLayout::*`, `content::MeterLayout::*`, `content::RecipeLayout::*`, `content::WorkshopLayout::*`, `content::MenuLayout::*`, `content::LootLayout::*`, `content::FillTableLayout::*`, `content::ContainerLayout::*`, `content::ImageLayout::*`, `content::RecipeEdit::*`, `content::ResourceType::*`, `content::Host::game_resource`, `content::game_resource`, `content::Load::serve`, `content::Load::allow_rebuild`, `content::rebuild`, `content::add_recipe_to`, `content::change_fill_table`, `content::change_random_loot`, `content::change_recipes_in`, `content::clone_image`, `content::clone_item`, `content::clone_meter_template`, `content::clone_station`, `content::clone_template`, `content::derive_recipes_in`, `content::fill_items`, `content::list_item`, `content::list_meter_template`, `content::list_recipes`, `content::list_template`, `content::list_workshop`, `content::move_recipes`, `content::recipe_workshops`, `content::resize_container`, `content::template_table` | A resource's bytes and their layouts. Luau reads and serves the same resources as values: `enshwick.resources.game_value`, `serve` and `serve_new`. |
| `content::Vanilla::game`, `content::Vanilla::new` | The vanilla content as a Rust value. Luau reaches its lookups as `enshwick.content` functions. |
| `content::RecipeRule::*` | Luau names a rule as a table: `{ Recipe = name }`, `{ Output = name }`, `{ Workshop = name }` or `"Hand"`. |
| `files::read_if_present`, `files::read_to_string_if_present` | A script reads only its own folder, with `enshwick.mods.read`, and keeps a world's data with `enshwick.state`. It writes any file with `enshwick.files.write_atomically`. |
| `loot::Containers::*`, `loot::LootPass::*` | Reads of the game's context inside a system, which lives only during the call. Luau's `on_loot_pass` handler gets the pass's containers as a list. |
| `inventory::Inventories::read`, `inventory::Inventories::read_in` | A reader borrowed for one call. Luau reads through `enshwick.inventory` itself: `slot`, `root`, `durability` and `meter`. |
| `stats::Stat::from_ref`, `stats::Def::to_def` | A stat from the binary interface's reference or the loader's own table, which a script never holds. |
| `world::World::component` | A pointer into the game's memory. |
| `status::Load::feature`, `state::ModLocal::*` | Rust's own init and statics. A script's start is its `main.luau`, and it keeps values in its own locals. |
| `resources::Type::*`, `resources::Field::*`, `resources::Attribute::*`, `resources::Constant::*`, `resources::Kind::*`, `resources::Registry::*`, `resources::TypeTable::*`, `resources::Variant::*`, `resources::CodecError::*` | The reflected types as Rust values. Luau gets a type as the table `enshwick.resources.named` gives: its names, hashes, layout, fields, constants and attributes; a variant is `{ type = ..., value = ... }`. |
| `game_events::EventType::*`, `game_events::RawGameEvent::*`, `game_events::Load::game_event_types`, `Load::game_event_type`, `Load::on_raw_game_event`, `Load::on_every_raw_game_event`, `Load::on_raw_event`, `Load::type_size` | A game event by its reflected type, read field by field in Rust. Luau names the fields and their kinds: `enshwick.on_game_type`, and `enshwick.game_event_types` lists the types. |
| `ecs::Ecs::*`, `ecs::Components::*`, `ecs::Attributes::*`, `ecs::Query::*`, `ecs::Worlds::*`, `ecs::server_role`, `ecs::transform_position` | Reads and writes inside a hooked system, through the system's context, which only native code holds. |
| `time::*` | The clock a mod's own native systems advance. A script reads the time from the `Ticked` event and `enshwick.daytime`. |
| `effects::Effects::emit_raw` | The loader raises only the event types the typed calls raise, and a script has those calls: `npc_command`, `set_toggle`, `add_threat`, `ping`, `teleport` and `spawn`. |
| `Host::build`, `Host::hosting`, `Host::log_level` | A script holds no `Host`: `enshwick.mods.side`, `enshwick.mods.hosting` and `enshwick.log.level` answer the same. |
| `Load::publish`, `Load::service`, `Load::optional_service` | A service is a table of native functions. A Luau mod offers and uses another mod's work through calls and events: `enshwick.serve_call`, `enshwick.caller`, `enshwick.define_event` and `enshwick.on_mod_event`. |
| `services::Caller::call`, `services::Emitter::send` | Luau's caller is the function `enshwick.caller` returns, and its emitter the table `enshwick.define_event` returns, with `send`. |
| `services::DescribedCall::*`, `services::DescribedCaller::*`, `services::DescribedEvent::*`, `services::DescribedEmitter::*`, `services::Load::define_described_event`, `services::Load::described_caller`, `services::Load::on_described_mod_event`, `services::Load::serve_described_call`, `services::compare`, `services::layout_hash`, `services::read_layout` | The layouts and hashes the runtime builds a script's calls and events from. A script writes the description as a table, and the runtime lays it out and hashes it. |
| `points::Load::extend_point`, `points::Load::observe_point`, `points::Load::extend_described_point`, `points::DescribedPoint::*`, `points::DescribedField::*` | A point's input and answer are Rust types. A script links with `enshwick.extend(point, handler, order)`, where `point` is the module `cargo enshwick deps --luau` writes with the point's layout hashes; the point's mode decides whether the link changes the answer or observes it. |
| `points::Load::define_point`, `points::OwnedPoint::*`, `points::Decision::*` | A point's owner declares it from a Rust type with `#[point]`, and its manifest carries the schema other mods link against. A pack declares no points, so a Luau mod links into points but owns none. |
| `config::*` | Settings from a Rust type or a schema built in code. A Luau mod declares its settings in `pack.toml`, which the loader reads into the same schema; `enshwick.settings()` returns the values in force, and `SettingsChanged` says when they change. |
| `packs::*` | Reading a pack's folder and files. The loader reads a Luau mod's own folder as its pack, and a script adds content from code through `enshwick.content` and `enshwick.affixes`. |
| `typed::EventType::*`, `typed::ResourceType::*` | Rust types generated from the game's reflection, which a Rust mod compiles in. Luau reaches the same events with `enshwick.on_game_type` and the same resources as values with `enshwick.resources.game_value` and `serve`; `cargo enshwick types --luau` writes their types for the editor. |
| `affixes::roll` | `enshwick.affixes.roll` rolls an item, as `Affixes::roll` does, since a namespace holds one function of a name. The roll from a pool is `enshwick.affixes.roll_pool`. |
| `actions::clean`, `actions::decode`, `actions::decode_args`, `actions::encode`, `actions::encode_args` | The text form a sheet and its arguments take between the server and a client. The loader and the fake host speak it; a script's actions never see it. |
| `controls::Key::bits`, `controls::Key::from_bits`, `ui::Response::from_raw`, `ui::Response::to_raw` | The binary interface's form of a key and of a dialog's answer. A script names a key by its text, such as `Shift+E`, and an answer by its name. |
| `controls::Tracker::*` | The key tracker the loader's key reader and the fake host share, so both fire a control alike. A script's control fires through the same loader. |
| `ui::Ui::ask` | It blocks a thread of the mod's own until the player answers. A script has no thread of its own, so it takes a dialog's answer in a callback, as `Ui::dialog` does. |
| `affixes::lines_of` | The loader's own listing of an item's lines from the pool it keeps. A script reads them with `enshwick.affixes.lines`. |

## The sandbox

A script reaches the game only through `enshwick`. It has Luau's own
libraries for tables, strings, `utf8`, `bit32`, `math`, `buffer`, `vector`,
coroutines and the clock (`os.clock`, `os.time`, `os.date`), and nothing
that starts processes or loads compiled code. It reads only its own folder,
with `enshwick.mods.read`, and writes files with
`enshwick.files.write_atomically`, such as exports for a wiki or a map into
`enshwick.export_dir()`, the folder `export/<folder>/` in the game folder,
named as the mod's folder in `mods/`.
A write may name any path the game may write, as a Rust mod's may, so the
sandbox keeps a script's mistakes small but is no security boundary: a Luau
mod needs the same trust as a Rust mod.
The globals and the `enshwick` tables are read-only. `require` resolves only
the mod's own scripts.

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

`main.luau` runs on the loader's start thread. Event handlers and tasks run
on the server's game thread, between simulation steps, where calls into the
world are allowed (see [Threads](concepts/threads.md)). A hook or filter a
script sets, such as `enshwick.damage.on_damage`, runs where the Rust one
runs, inside the game's step; the mod's scripts run one call at a time, and
each call counts against the budgets, so keep such a function short.

## What a mod costs

With `diagnostics = true`, the status summary shows each Luau mod's memory,
its peak, its calls and how many ran past their budget, beside the time its
handlers took (see [Logging and diagnostics](logging.md)).

<!--
type: how-to
description: A Luau mod is a data pack with scripts: pack.toml and main.luau in a folder. It uses the Rust API's modules, names and types, with its differences listed.
-->

# Luau mods

A Luau mod changes the game with scripts and needs no compiler. It is a
[data pack](content/packs.md) with scripts: a folder in the game's mods
folder with a `pack.toml` and a `main.luau`.

## What a Luau mod can do

A Luau mod reaches the API a Rust mod has, through the global `enshwick`.
Each module of the Rust API is a table of it, with the Rust names:
`enshwick.<module>.<function>`. Each chapter of this book shows the Luau
beside the Rust. Where Luau differs, [the table of
differences](#where-luau-differs-from-rust) says why.

Scripts are [Luau](https://luau.org), Roblox's typed dialect of Lua 5.1.
Each mod runs in a sandbox of its own and reaches the game only through
`enshwick`.

## How the loader runs it

The loader runs a Luau mod as it runs any pack. It is in the mod list and
the lock, and it counts in [the join check](concepts/join-check.md) with
the hash of every script. It has its own log,
`logs/<id>_<date>_<time>.log`, and when it goes wrong, the loader turns it
off alone.

`main.luau` may load other `.luau` files of the folder, up to four folders
deep. The folder may hold data files too, such as `items.toml`:

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

## Start a Luau mod

[Hello, world](getting-started.md#a-luau-script) shows the smallest Luau
mod. To start one of your own:

1. Run `cargo enshwick new --luau you.greeter`. It makes the folder with a
   `main.luau` that greets each player who joins, a `pack.toml` and a
   README. `--side` sets where it runs, the server when left out.
2. Open the folder in an editor with the
   [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp) extension. The
   folder's `.vscode/settings.json` points it at the definitions in
   `types/`, so every call and event shows its types and documentation.
3. Run `cargo enshwick check` in the folder. It reads the mod as the loader
   does, then type-checks every script with luau-lsp against the definitions
   in `types/`, without the game. It prints each problem with its file, line
   and column, and exits with 1 on an error. A misspelt event gives an error
   on its line and one listing every event `enshwick.on` takes:

   ```text
   main.luau:6:13: error: Expected this to be '"SettingsChanged"' but got '"PlayerJoind"'
   main.luau:6:1: error: Other overloads are also not viable: ("LoaderSettingsChanged", () -> ()) -> (); ("WorldLoaded", () -> ()) -> (); ...
   error: you.greeter: 3 errors
   ```

   Without luau-lsp on the path, `check` downloads the release it was
   built for once, checks its SHA-256 and keeps it in the user's cache
   folder. `--offline` skips the download and leaves the scripts unchecked.
4. Copy the folder into the game's `mods/` folder as `mods/you.greeter/`.

The repository's
[`examples/you.welcome`](https://github.com/enshwick/enshwick/tree/main/examples/you.welcome)
is a Luau mod to start from.

`types/` holds the definitions `new` wrote:

| File | Declares |
| --- | --- |
| `enshwick.d.luau` | the API, for the loader the tool was built with |
| `enshwick.typed.d.luau` | the values of [`enshwick.typed`](game-types.md#can-a-luau-mod-use-them) |
| `game.d.luau` | the [game's own types](game-types.md), such as the fields `enshwick.on_game_type` hands a handler; only with `--game <game folder>` |

After an update, write them again:

- `cargo enshwick luau --out types/enshwick.d.luau --typed types/enshwick.typed.d.luau`
  for the API and the typed values;
- `cargo enshwick types --from <game folder> --luau types/game.d.luau` for
  the game's types, then add `--definitions:@game=types/game.d.luau` to the
  check.

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

Names are the Rust API's. `enshwick.world.notify_all` is
`World::notify_all`. The event `PlayerJoined` is `events::PlayerJoined`.
The Rust shapes map this way:

- A handle a Rust mod gets from `Load` is its module's table. For
  `load.weather()`, a script calls `enshwick.weather.at(position)`.
- On a game build without what a handle needs, the table keeps only its
  start-time functions, and the log says why. A script checks the function
  it calls first: `if enshwick.weather.at then`.
- Some handles switch a loader feature on, such as the altars, the stats
  and the map's fog. A script opens one at the top level of `main.luau`, as
  a Rust mod asks for it while it starts. `enshwick.altars.altars()` returns
  it as an object. Until then the handle's methods raise an error, and the
  loader arms none of the hooks behind them.
- A call a Rust mod makes on `Load` while it starts is a function a script
  calls at the top level of `main.luau`. `load.define_affix(...)` becomes
  `enshwick.affixes.define_affix(...)`, and after the start it raises an
  error.
- A Rust handle a call returns, such as a `ClockSpeed`, is a Luau object
  with the handle's methods: `speed:set(2)`. A method that takes `self` by
  value returns the next object, as a builder does.
- A hook or filter takes a function. The function gets the value Rust hands
  a closure, and returns the changed value, or nothing to keep it.

These belong to the `enshwick` table itself:

| Name | What |
| --- | --- |
| `enshwick.on(event, handler)` | calls `handler` on each loader [event](events.md) or typed [game event](game-events.md#from-luau), with its data as a table; only at the top level of `main.luau` |
| `enshwick.on_game_type(type, fields, handler)` | a game event by its reflected type, with the fields named |
| `enshwick.extend(point, handler, order)` | a link into another mod's [extension point](extending.md#from-luau) |
| `enshwick.points.define_point(name)` | an [extension point](extending.md#defining-a-point-in-luau) of the mod's own, declared in `pack.toml` |
| `enshwick.on_mod_event(event, handler)`, `enshwick.define_event(event)` | [mod events](services.md#mod-events), heard and sent |
| `enshwick.caller(call)` | a function that runs [another mod's call](calls.md#from-luau); unstable |
| `enshwick.deps` | the mods the pack's `requires` and `after` tables name, [by alias](services.md#other-mods-by-alias) |
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
| `&str`, `String`, a GUID, a [content id](concepts/ids.md), a path | `string` |
| a position, `[f64; 3]` | `vector` |
| bytes, `&[u8]` | `buffer` |
| `Option<T>` | `T?` |
| `Vec<T>` | `{T}` |
| a pair, such as `(String, String)` | a list of the two, `{ a, b }` |
| a struct | a table with its fields |
| an enum such as `Notice` | the variant's name, `"OnScreen"`, or `{ Name = value }` for a variant with a value |
| a Rust handle | an object with its methods |
| `Duration` | seconds |

The Luau API is read from descriptions kept beside the Rust items, so it
is no second copy. `#[derive(Script)]` describes a type.
`#[script(<part>)]` marks the methods of an impl block a script may call. The
event tables describe each event. A new event or a newly marked method
reaches Luau and the definitions with no other change.

A call that fails raises a Luau error with the call's name and the reason,
such as `enshwick.world.broadcast: text: expected a string, got a number`.
`pcall` catches it.

## Where Luau differs from Rust

Every public function of the Rust API has a Luau function of the same
module and name, `enshwick.<module>.<name>`. A Rust handle's method is a
method of the Luau object, `handle:<name>()`.

A value, such as an item or a change, is a table with the struct's fields,
without the struct's methods. Where Rust has `Container::is_open`, a script
reads the fields that method reads.

A test of the repository fails for any other function without a Luau
mirror, unless this table names it and says why.

| Rust | Why |
| --- | --- |
| `abi::*` | The binary interface between the loader and a mod's DLL. A Luau mod has no DLL: its runtime crosses the interface for it. |
| `raw::*`, `code::*`, `Hook::*`, `Frame::*`, `Load::hook`, `Load::hook_at`, `Load::code`, `Load::find_code`, `Load::patch_code`, `Load::symbol`, `Load::constant`, `Load::field_offset`, `Load::type_size`, `Load::system` | Native code: detours, code patches, byte patterns and raw addresses run machine code in the game's own frames. A script runs in a sandbox without pointers. |
| `Host::api`, `Host::handle`, `Host::caller_frame`, `Host::fault`, `host` | The raw handle to the loader. The runtime holds it for the script. |
| `script::*` | The script binding itself, which the Luau runtime runs on. |
| `test::*` | The fake loader a Rust mod's tests run against. A Luau mod is checked with `cargo enshwick check` and on a server. |
| `shape::*` | Layout hashes of Rust types. The runtime hashes a script's descriptions itself. |
| `Off::new`, `Off::text` | Luau reports a failure with `error()`, and a caller catches it with `pcall`. |
| `sound::Load::change_sound`, `sound::Load::add_sound` | Luau calls them by their module, `enshwick.sound.change` and `enshwick.sound.add`; a container is its GUID as text. |
| `content::Blob::*`, `content::ItemLayout::*`, `content::RecipeLayout::*`, `content::WorkshopLayout::*`, `content::MenuLayout::*`, `content::LootLayout::*`, `content::FillTableLayout::*`, `content::ContainerLayout::*`, `content::ImageLayout::*`, `content::RecipeEdit::*`, `content::ResourceType::*`, `content::Host::game_resource`, `content::Host::served_resource`, `content::game_resource`, `content::Load::serve`, `content::Load::allow_rebuild`, `content::rebuild`, `content::add_recipe_to`, `content::at_workshop`, `content::change_fill_table`, `content::change_random_loot`, `content::change_recipes_in`, `content::clone_image`, `content::clone_item`, `content::clone_station`, `content::derive_recipes_in`, `content::fill_items`, `content::list_item`, `content::list_recipes`, `content::list_workshop`, `content::move_recipes`, `content::recipe_workshops`, `content::resize_container`, `content::template_table` | A resource's bytes and their layouts. Luau reads the same resources as values with `enshwick.resources.game_value`, and as served with `served_value`. It serves them with `serve` and `serve_new`. |
| `content::Vanilla::game`, `content::Vanilla::new` | The vanilla content as a Rust value. Luau reaches its lookups as `enshwick.content` functions. |
| `content::RecipeRule::*` | Luau names a rule as the table `{ Recipe = name }` or `{ Output = name }`. It also takes the table `{ Workshop = name }` or `"Hand"`. |
| `files::read_if_present`, `files::read_to_string_if_present` | A script reads only its own folder, with `enshwick.mods.read`, and keeps a world's data with `enshwick.state`. It writes any file with `enshwick.files.write_atomically`. |
| `loot::Containers::*`, `loot::LootPass::*` | Reads of the game's context inside a system, which lives only during the call. Luau's `on_loot_pass` handler gets the pass's containers as a list. |
| `inventory::Inventories::read`, `inventory::Inventories::read_in` | A reader borrowed for one call. Luau reads through `enshwick.inventory` itself. Its calls are `slot` and `root`. It also has `durability` and `meter`. |
| `stats::Stat::from_ref`, `stats::Def::to_def` | A stat from the binary interface's reference or the loader's own table, which a script never holds. |
| `world::World::component` | A pointer into the game's memory. |
| `vetoes::BuildVerdict::refuse_because` | A Luau build filter refuses with a reason by returning the reason as a string. |
| `status::Load::feature`, `state::ModLocal::*` | Rust's own init and statics. A script's start is its `main.luau`, and it keeps values in its own locals. |
| `resources::Type::*`, `resources::Field::*`, `resources::Attribute::*`, `resources::Constant::*`, `resources::Kind::*`, `resources::Registry::*`, `resources::TypeTable::*`, `resources::Variant::*`, `resources::CodecError::*` | The reflected types as Rust values. Luau gets a type as the table `enshwick.resources.named` gives: its names, hashes, layout, fields, constants and attributes; a variant is `{ type = ..., value = ... }`. |
| `game_events::EventType::*`, `game_events::RawGameEvent::*`, `game_events::Load::game_event_types`, `Load::game_event_type`, `Load::on_raw_game_event`, `Load::on_every_raw_game_event`, `Load::on_raw_event`, `Load::type_size` | A game event by its reflected type, read field by field in Rust. Luau names the fields and their kinds: `enshwick.on_game_type`, and `enshwick.game_event_types` lists the types. |
| `ecs::Ecs::*`, `ecs::Components::*`, `ecs::Attributes::*`, `ecs::Query::*`, `ecs::Worlds::*`, `ecs::server_role`, `ecs::transform_position` | Reads and writes inside a hooked system, through the system's context, which only native code holds. |
| `time::*` | The clock a mod's own native systems advance. A script reads the time from the `Ticked` event and `enshwick.daytime`. |
| `effects::Effects::emit_raw` | The loader raises only the event types the typed calls raise, and a script has those calls: `npc_command` and `set_toggle`. It also has `add_threat` and `ping`. It has `teleport` and `spawn` too. |
| `commands::Caller::*`, `commands::run_later` | A script names a remote caller by its label and its permissions, as the arguments of `enshwick.commands.run`. A script runs on the server's game thread, so it has no `run_later`. |
| `Host::build`, `Host::hosting`, `Host::log_level` | A script holds no `Host`. `enshwick.mods.side` and `enshwick.mods.hosting` answer the same. So does `enshwick.log.level`. |
| `Load::publish`, `Load::service`, `Load::optional_service` | A service is a table of native functions. A Luau mod offers and uses another mod's work through calls and events. It serves a call with `enshwick.serve_call` and runs one with `enshwick.caller`. It sends events through `enshwick.define_event` and hears them with `enshwick.on_mod_event`. |
| `services::Caller::call`, `services::Emitter::send` | Luau's caller is the function `enshwick.caller` returns. Its emitter is the table `enshwick.define_event` returns, with `send`. |
| `services::DescribedCall::*`, `services::DescribedCaller::*`, `services::DescribedEvent::*`, `services::DescribedEmitter::*`, `services::Load::define_described_event`, `services::Load::described_caller`, `services::Load::on_described_mod_event`, `services::Load::serve_described_call`, `services::compare`, `services::layout_hash`, `services::read_layout` | The layouts and hashes the runtime builds a script's calls and events from. A script writes the description as a table, and the runtime lays it out and hashes it. |
| `points::Load::extend_point`, `points::Load::observe_point`, `points::Load::extend_described_point`, `points::Load::define_described_point`, `points::DescribedPoint::*`, `points::DescribedField::*`, `points::DescribedOwnedPoint::*` | A point's input and answer are Rust types. A script links with `enshwick.extend(point, handler, order)`. There `point` is the module `cargo enshwick deps --luau` writes with the point's layout hashes. The point's mode decides whether the link changes the answer or observes it. A script's own point is a `[[point]]` of `pack.toml`, which the runtime lays out and hashes. |
| `points::Decision::*` | `decide` returns two values in place of the enum: the answer and a word for what the links did. That word is "kept", "changed" or "refused", with nil for the answer after a refusal. |
| `config::*` | Settings from a Rust type or a schema built in code. A Luau mod declares its settings in `pack.toml`, which the loader reads into the same schema. `enshwick.settings()` returns the values in force, and `SettingsChanged` says when they change. |
| `packs::*` | Reading a pack's folder and files. The loader reads a Luau mod's own folder as its pack, and a script adds content from code through `enshwick.content` and `enshwick.affixes`. |
| `typed::ComponentType::read`, `typed::ComponentType::write` | The handle `enshwick.typed.component_type` returns holds the server's world from the start. So its `read` and `write` take no world. A value is a table by the Rust item's field names, and a write names every field ([The game's own types](game-types.md#can-a-luau-mod-use-them)). |
| `affixes::roll` | `enshwick.affixes.roll` rolls an item, as `Affixes::roll` does, since a namespace holds one function of a name. The roll from a pool is `enshwick.affixes.roll_pool`. |
| `world::near` | `enshwick.world.near` is `World::near`, the entities near a point, since a namespace holds one function of a name. A script compares the offset itself, as the Luau example of [Loot containers](content/loot.md) does. |
| `actions::clean`, `actions::decode`, `actions::decode_args`, `actions::encode`, `actions::encode_args` | The text form a sheet and its arguments take between the server and a client. The loader and the fake host speak it; a script's actions never see it. |
| `ui::Response::from_raw`, `ui::Response::to_raw` | The binary interface's form of a dialog's answer. A script names an answer by its name. |
| `controls::Tracker::*` | The key tracker the loader's key reader and the fake host share, so both fire a control alike. A script's control fires through the same loader. |
| `actions::Action::*`, `actions::Window::*`, `actions::Row::*`, `controls::Control::*`, `ui::Label::*`, `ui::Button::*`, `ui::Toggle::*`, `ui::Slider::*`, `ui::Dropdown::*`, `ui::Dialog::*`, `ui::Marker::*` | Builders. A script writes the value as a table of the builder's fields, such as `{ text = "Saved", style = "Positive" }` for a label. A slider's range is its `min` and `max`. In an action's table, `allow` and `at` are functions. So are `per_player` and `known`. A window is `{ id, title, items }`. An item is `{ Text = { key, values } }` or `{ Row = { text, buttons } }`. It can also be `{ Action = id }`. |
| `actions::FindSubjectError::reason` | An action's `at` answers as the Rust `Result` does. It returns `{ Ok = subject }` or `{ Err = reason }`, so the reason is the text itself. |
| `controls::Key::parse`, `controls::Key::from_bits`, `controls::Key::bits`, `controls::Key::code`, `controls::Key::modifiers`, `controls::Key::is_mouse` | A key crosses as its text, such as `Shift+E`. `enshwick.controls.parse` and `from_bits` return the key's own spelling. `bits` and `code` take the text. So do `modifiers` and `is_mouse`. |
| `ui::Frame::toggle`, `ui::Frame::slider`, `ui::Frame::dropdown` | Lua has no `&mut`. A script hands the value in and gets it back, changed in the frame the player changes it. |
| `ui::HudScreenType::name` | A screen crosses as its name, such as `"StorageChest"`. |
| `ui::Ui::ask` | It blocks a thread of the mod's own until the player answers. A script has no thread of its own, so it takes a dialog's answer in a callback, as `Ui::dialog` does. |
| `affixes::lines_of` | The loader's own listing of an item's lines from the pool it keeps. A script reads them with `enshwick.affixes.lines`. |

A few calls cost more in Luau than in Rust. The filter of
`enshwick.interactions.on_interaction` runs for every entity in each
player's reach on every frame. A Rust filter allocates nothing there, but
the runtime builds the interaction's table and enters the script each time.
Keep that filter short, or write it in Rust for a mod that checks many
entities.

## The sandbox

A script reaches the game only through `enshwick`. The globals and the
`enshwick` tables are read-only, and `require` resolves only the mod's own
scripts.

A script has Luau's own libraries for tables, strings, UTF-8, bits, math,
buffers, vectors and coroutines. Of the `os` library it has the clock,
time and date functions. A script has nothing that starts processes or
loads compiled code.

A script reads only its own folder, with `enshwick.mods.read`. It writes
files with `enshwick.files.write_atomically`. Exports for a wiki or a map go
into `enshwick.export_dir()`. That is the folder `export/<id>/` in the
game folder, named by the mod id. A Luau mod's folder in `mods/` has the
same name.

The sandbox keeps a script's mistakes small, but it is no security
boundary. A write may name any path the game may write, as a Rust mod's
may. So a Luau mod needs the same trust as a Rust mod.

Each mod has these budgets:

| Budget | Limit | Past it |
| --- | --- | --- |
| memory | 64 MiB for the mod's whole VM | the allocation fails |
| time per call | 10 ms per call into the scripts, 2 s for `main.luau` at start | the script stops; `pcall` cannot catch it |
| time per second | 100 ms for all calls into the scripts within one second | the mod turns off |

A server step takes about 50 ms. So a handler that runs on every step should do
little work each time and leave the rest to a task.

## When a script fails

A script that fails turns its mod off, as a panic turns off a Rust mod. It
fails when an error leaves a handler or a task, when it runs past a budget,
or when a call it made panics. The log names the file and line:

```text
WorldSaved: main.luau:2: boom
```

The game and every other mod run on. A `main.luau` that does not compile or
fails at start leaves the mod off from the start, with the same line.

## Threads

`main.luau` runs on the loader's start thread. Event handlers and tasks run
on the server's game thread, between simulation steps, where calls into the
world are allowed (see [Threads](concepts/threads.md)).

A hook or filter a script sets, such as `enshwick.damage.on_damage`, runs
where the Rust one runs, inside the game's step. The mod's scripts run one
call at a time, and each call counts against the budgets, so keep such a
function short. Once warm, a damage filter's call allocates nothing. The
hit, its methods' arguments and a cause's name cross without the heap.
A table or text the script builds still allocates in Luau.

## What a mod costs

A script runs in Luau's interpreter, so it runs slower than the same logic in
a native mod. Write a mod whose code runs on every hit or frame in Rust, C or
C++. The measured costs are in [Performance](concepts/performance.md).

With `diagnostics = true`, the status summary shows what each Luau mod
costs ([Logging and diagnostics](logging.md)). It lists the mod's memory
and its peak, its calls, how many ran past their budget, and the time its
handlers took.

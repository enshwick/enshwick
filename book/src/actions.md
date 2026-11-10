<!--
type: how-to
description: A mod declares what players and administrators can do as actions, and the platform offers them on the Mods screen, from the mod's controls and on the console.
-->

# Player actions

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

A mod declares what players and administrators can do as actions; the
server checks and runs them wherever they come from.

## Where actions run from

A mod declares each thing players or administrators can do once, as an
action. Examples are letting a friend open a chest or picking a reward. The
mod offers the action through means of its own:

- from the mod's own [controls](controls.md) and
  [panels on the game's screens](interface.md#panels-on-the-games-screens),
  which send one with `Actions::run`;
- on the server's [console](chat.md#the-console), as the mod's id, the
  action and its values, such as a line typed in a dedicated server's
  console window;
- in a [window the server shows](#windows-from-the-server) on the
  player's client, which needs only Enshwick there.

The Mods screen shows a mod and its settings, not its actions.

A mod may also name its actions as [commands](commands.md), a second way to
run them from chat, the console and remote consoles. Single player has no
chat.

## A first action

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let texts = load.texts_from("[en]\n\"set\" = \"Change the weather\"\n")?;
    let actions = load.actions(texts)?;
    let kinds = [("clear", "Clear"), ("rain", "Rain"), ("storm", "Storm")];
    actions.add(
        Action::new("set", "set")
            .admins()
            .arg(Arg::choice("Weather", &kinds))
            .arg(Arg::number("Minutes", 1.0..=60.0, 1.0)),
        move |request| match request.args {
            [Value::Choice(kind), Value::Number(minutes)] => {
                Outcome::done(format!("{} for {minutes} min", kinds[*kind].1))
            }
            _ => Outcome::refused("Pick a weather"),
        },
    )?;
    Ok(())
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.texts_from('[en]\n"set" = "Change the weather"\n'))
local kinds = { { "clear", "Clear" }, { "rain", "Rain" }, { "storm", "Storm" } }
actions:add({
    id = "set",
    label = "set",
    admins = true,
    args = {
        enshwick.actions.choice("Weather", kinds),
        enshwick.actions.number("Minutes", 1, 60, 1),
    },
}, function(request)
    local args = request:args()
    local kind, minutes = args[1].Choice, args[2].Number
    if kind == nil or minutes == nil then
        return enshwick.actions.refused("Pick a weather")
    end
    return enshwick.actions.done(`{kinds[kind + 1][2]} for {minutes} min`)
end)
```

With the mod's id `weather`, the console line `weather set storm 10` runs
it. `weather help` lists the actions. A control of the mod runs it with
`actions.run("set", &[Value::Choice(2), Value::Number(10.0)])`.

## Declaring

[`load.actions(texts)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/actions.rs)
returns the mod's `Actions`. Call it once,
during `init`, on both sides of a mod of side `both` alike. Texts are keys of
`texts`, the mod's [translations](settings.md), shown in each player's
language. A key without a text shows as it is, so plain English works too.
The loader keeps these texts for the mod's controls as well. They sit beneath
the texts the mod hands it with `load.texts` or `load.texts_from`, before or
after. A key both give reads as the mod handed it. A language without the
text falls back as any text does: Brazilian Portuguese falls back to
Portuguese, then to English.
`actions.add(action, handler)` declares one action:

| Builder                             | What it does                                                                                     |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| `Action::new(id, label)`            | a button; `id` is 1 to 32 of `[a-z0-9-]`, its name on the console                                |
| `Action::per_player(id, label, on)` | a list of players with a switch each; `on` names the players switched on                         |
| `.help(key)`                        | a line under the label, and what `help <id>` answers on a typed line                             |
| `.group(key)`                       | the group the [help](commands.md#help-and-mistyped-lines) lists it under, and a window shows over it |
| `.with_self()`                      | its players include the player who looks, first; without it they hold only the others            |
| `.arg(Arg::...)`                    | an argument, in order; its kinds follow this table                                               |
| `.allow(fn)`, `.admins()`           | who sees the action and may run it                                                               |
| `.needs(permission)`                | only players the game's user groups give that [permission](commands.md#limit-an-action-to-a-permission) |
| `.at(fn)`                           | what it acts on for a player, or why there is nothing, which the player reads beside it          |
| `.known(fn)`                        | accounts the mod knows beyond the players online, for player arguments and lists                 |

`Arg` makes five kinds of argument:

- `player`
- `choice`
- `number`
- `toggle`
- `text`

In Luau, `enshwick.actions.actions(texts)` returns the `Actions`. An action
is a table of the builder's fields:

- `id`
- `label`
- `help`
- `group`
- `with_self`
- `args`
- `admins`
- `needs`, a list of permissions such as `{ "CanEditBase" }`
- the function `allow(player)`
- the function `at(ask)`
- the function `per_player(ask, subject)`
- the function `known(ask)`

Only `true` from `allow` lets a player in. With `admins` set, both must
hold. Five functions make the arguments:

- `enshwick.actions.player`
- `choice`
- `number`. It takes the range as `min` and `max`.
- `toggle`
- `text`. It takes the most bytes as a number.

`at` returns `{ Ok = enshwick.actions.subject(id, label) }` on success. On
failure it returns `{ Err = reason }`. `per_player` returns a list of
`{ key, name }`. The function `known` returns one too. A subject's `id` is a 64-bit key, so it crosses as its decimal digits.

```luau
local actions = enshwick.actions.actions(enshwick.i18n.texts_from('[en]\n"access" = "Who may use it"\n'))
local allowed = {}
actions:add({
    id = "access",
    label = "access",
    at = function(ask)
        if ask:player().entity == 0 then
            return { Err = "No locker of yours here" }
        end
        return { Ok = enshwick.actions.subject("42", "Your locker") }
    end,
    per_player = function(ask, subject)
        return allowed
    end,
}, function(request)
    local args = request:args()
    if args[2].Toggle then
        table.insert(allowed, { key = args[1].Player.key, name = args[1].Player.name })
    end
    return enshwick.actions.done(args[1].Player.name)
end)
```

Each argument is one of five kinds:

- a player online or known to the mod, named on the console;
- one of a few options;
- a number in a range with a step;
- on or off;
- a line of text of 1 to its most bytes, at most 256, without a control
  character, such as a ban reason or an announcement.

A list takes the players online and the ones `on` names, so a player
switched on can be switched off while away. A list and a player argument
offer the other players. With `.with_self()` they offer the player who
looks too, first, for a switch or an action a player uses on themselves.

### Optional arguments

`Arg::optional(default)` lets a typed line leave an argument out. It then
takes `default`. In Luau it is the method `:optional(value)` of an argument.

| Rule | Detail |
| --- | --- |
| Which arguments | only the last ones of an action; `actions.add` refuses any other |
| Which kinds | a choice, a number, on or off, a text; never a player. A text only as the last argument |
| The default | a value the argument takes, or an empty text |
| In the help | the argument stands in square brackets: `give <player> <Item> [1-999]` |
| From the interface | a window opens the field with its default; values left out at the end take their defaults too |

A text or a player that only optional arguments follow takes the rest of
the line up to them. The last words go to the optional arguments, in
order, as far as each reads as its argument:

| The line, for `give <player> <Item> [1-999]` | The item | The count |
| --- | --- | --- |
| `give Ann Stone Axe` | `Stone Axe` | 1 |
| `give Ann Stone Axe 5` | `Stone Axe` | 5 |
| `give Ann 5` | `5` | 1 |
| `give Ann "Stone 5"` | `Stone 5` | 1 |
| `give Ann Stone Axe 5000` | `Stone Axe 5000` | 1 |

An optional text is always the last argument, and takes what is left of the
line itself. The arguments before it then take one word each, or a run in
double quotes:

| The line, for `warn <player> [1-60] [Reason]` | The minutes | The reason |
| --- | --- | --- |
| `warn Ann` | the default | none |
| `warn Ann 5 spamming a lot` | 5 | `spamming a lot` |
| `warn Ann spamming 5` | the default | `spamming 5` |

```rust
use enshwick::actions::{Action, Arg, Value};

let give = Action::new("give", "Give an item")
    .admins()
    .arg(Arg::player("Player"))
    .arg(Arg::text("Item", 64))
    .arg(Arg::number("Count", 1.0..=999.0, 1.0).optional(Value::Number(1.0)));
assert_eq!(give.id(), "give");
```

```luau
local give = {
    id = "give",
    label = "Give an item",
    admins = true,
    args = {
        enshwick.actions.player("Player"),
        enshwick.actions.text("Item", 64),
        enshwick.actions.number("Count", 1, 999, 1):optional({ Number = 1 }),
    },
}
```

## Items from the game's list

`Arg::item(label)` asks for one of the game's items. The handler gets
`Value::Item` with the item's GUID. The server refuses an item its game
does not know before the handler runs: "No item …".

| Where | How the player names the item |
| --- | --- |
| A window | a list of the items by the names the game shows in the player's language, with a search field; up to 64 matches show at once |
| A typed line | its debug name (`Tool_Torch01`), its GUID, a content id (`you.lamps:lamp`), or a name the game shows where the server has the game's texts, as in single player and a game a player hosts |

The names come from the player's own game, so a dedicated server needs no
texts and sends none. A window lists vanilla items; a typed line also takes
a mod's item by its content id. On a typed line the item takes words as a
text does: `spawn Stone Axe 5` is the item `Stone Axe` five times.

```rust
use enshwick::actions::{Action, Arg, Value};

let spawn = Action::new("spawn", "Spawn an item")
    .admins()
    .arg(Arg::item("Item"))
    .arg(Arg::number("Count", 1.0..=999.0, 1.0).optional(Value::Number(1.0)));
assert_eq!(spawn.id(), "spawn");
```

```luau
local spawn = {
    id = "spawn",
    label = "Spawn an item",
    admins = true,
    args = {
        enshwick.actions.item("Item"),
        enshwick.actions.number("Count", 1, 999, 1):optional({ Number = 1 }),
    },
}
```

## Players who are offline

An administrator bans or allows players who are not online. `.known(fn)`
names the accounts the mod knows beyond the players online. Examples are
banned players or those refused at the door, newest first. Each is an
account key and the last name the mod saw. The function runs on the server
each time a player looks at or runs the action.

- The action's player arguments and its list offer the players online
  first, then the known accounts, 32 rows at most.
- A name typed on the console, in a command or in a player's field finds
  any of the first 1,024 known accounts.
- A name that several accounts share is refused: "Several players are named
  Sam."
- A run hands the account's key in `Value::Player`, whether the player is
  online or not.

```rust
use enshwick::PlayerKey;
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::i18n::Texts;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(Texts::default())?;
    actions.add(
        Action::new("ban", "Ban")
            .admins()
            .arg(Arg::player("Player"))
            .arg(Arg::text("Reason", 120))
            .known(|_ask| vec![(PlayerKey::new(42), "Bob".into())]),
        |request| match request.args {
            [Value::Player { name, .. }, Value::Text(reason)] => {
                Outcome::done(format!("Banned {name}: {reason}"))
            }
            _ => Outcome::refused("Name a player"),
        },
    )?;
    // In the mod `you.bans`: `/bans` and `/b`.
    actions.commands("b")
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({
    id = "ban",
    label = "Ban",
    admins = true,
    args = { enshwick.actions.player("Player"), enshwick.actions.text("Reason", 120) },
    known = function(ask)
        return { { key = "42", name = "Bob" } }
    end,
}, function(request)
    local args = request:args()
    return enshwick.actions.done(`Banned {args[1].Player.name}: {args[2].Text}`)
end)
actions:commands("b")
```

In the mod `you.bans`, `/b ban Bob griefing on the coast` bans Bob while he
is offline.

## Handling

The handler runs on the server's game thread, where [world reads](world.md)
work. It gets a `Request` that holds:

- the player;
- the subject `.at` found for this run;
- the values picked, in the order of the arguments (a list hands
  `Value::Player` and `Value::Toggle`);
- the player's language;
- where it came from, its `Via`: the interface, the console, or a
  [command](commands.md) from chat or for a remote caller.

`request.say(key, values)` fills the mod's text in the player's language.
The handler returns an `Outcome`. It is `Outcome::done(text)` or
`Outcome::refused(text)`. From a control or panel, the text shows as one of
the game's notices. Its lines are joined into one and cut to a notice's
length. A client that takes no notices gets it as a chat line instead. From
the console, the text is the console's answer.

A Luau handler gets the request as an object whose methods read those
fields:

- `request:player()`
- `subject()`
- `args()`
- `language()`
- `via()`
- `say(key, values)`. Its `values` is a list of `{ name, value }`.

Each value is a table of one variant, such as `{ Number = 10 }`, and a
choice is its index from 0, as in Rust. The handler returns
`enshwick.actions.done(text)` or `enshwick.actions.refused(text)`; nil is
done without a text. A Luau client runs an action with
`actions:run(id, values)`.

`actions.control(id, key)` puts an action without arguments on a key in
one call. The control shows the action's label in each player's language.
In Luau it is `actions:control(id, key)`, and
[Controls](controls.md#changing-the-world) shows it.

The server decides everything. It checks `.allow` and runs `.at` again on
every run, whatever the client sent. It refuses values outside their
types and ranges before the handler sees them. A mod's own permission rules,
[extension points](extending.md) included, belong in `.at`, so the
mod's controls and panels and the console all follow them.

The platform's replies, such as "No player named Bob.", are under
`actions.*` keys and take the mod's texts over them.

## On a named object

A panel beside a chest's screen acts on that chest, not on whatever `.at`
finds nearest. `actions.run_at(id, values, target)` runs the action on the
object an `ActionTarget` names. A target names the object's entity in the
server's world, as the server sent it to the client. It can name where the
object stands in metres instead, or both. These make a target:

- `ActionTarget::entity(entity)`
- `ActionTarget::at(position)`
- `with_position`

The server hands it to `.at` as `ask.target`. The handler gets it as
`request.target`. A run without one, the console and the actions' list
have `None`.

```rust
use enshwick::actions::{Action, ActionTarget, Outcome, Subject};
use enshwick::i18n::Texts;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(Texts::default())?;
    actions.add(
        Action::new("sort", "Sort").at(|ask| match ask.target.and_then(|t| t.position) {
            Some(_) => Ok(Subject::new(1, "This chest")),
            None => Err("Open a chest first".into()),
        }),
        |_request| Outcome::done("Sorted"),
    )?;
    let chest = ActionTarget::at([10.0, 2.0, -4.5]);
    let _sent = actions.run_at("sort", &[], chest);
    Ok(())
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({
    id = "sort",
    label = "Sort",
    at = function(ask)
        if ask:target() == nil then
            return { Err = "Open a chest first" }
        end
        return { Ok = enshwick.actions.subject("1", "This chest") }
    end,
}, function() return enshwick.actions.done("Sorted") end)
actions:run_at("sort", {}, { position = vector.create(10, 2, -4.5) })
```

The target comes from the client, so the server checks it before the mod
sees the run. The loader drops a target whose position is not finite or
whose flags it does not know. A named entity must exist in the server's
world, and its position there replaces the one the client sent. The object
must stand within 12 metres of the player, enshwick's own bound; otherwise
the run is refused with "That is too far away." in the player's language.
Who may use the object is the mod's rule. That is why `.at` still checks that the
object is the kind it acts on and the player's to use.

## From the console

A console line names the mod by its id, then the action and its values as
words. Values are a player by name, a choice by its word, a number, `on`
or `off`, or a text. A list takes a player and `on` or `off`. For example:
`you.trading-post access Ann on`. The last text argument takes the rest of
the line as typed. A text argument before another value takes one word, or
a run in double quotes: `you.tags tag "red fox" 3`. The server refuses a
longer text than the argument takes: "Reason takes at most 120 bytes." The
last arguments may be [optional](#optional-arguments). `<mod id> help`
lists the actions with their words, and `<mod id> help <action>` explains
one ([Help and mistyped lines](commands.md#help-and-mistyped-lines)). The
mod's id in front keeps two mods' actions apart, see
[how ids are namespaced](concepts/ids.md). A mod's command name or alias
works in place of its id, as in `/adm online`. The console holds every
permission and has no character. That is why `request.player.entity` is 0
and `.admins()` lets it in. It is also why an `.at` that looks for something
near the player finds nothing. Answers are in English.

## Where it works

Actions run in the game server: the dedicated server, and a client while it
hosts a game, single player included. A control or panel that runs one
needs Enshwick on the player's client. A mod of side `server` has no part on
the client, so it offers its actions through controls that
[run an action](controls.md#a-mod-that-runs-on-the-server-only) and through
[windows](#windows-from-the-server). The server sends both to each player's
client.

## Windows from the server

A mod of side `server` opens windows on players' clients, though it has no
part there. The client's Enshwick draws them with the game's own widgets.
An administrator needs only Enshwick on their client to use them.

A `Window` holds a title and items, in order:

| Builder | What the player sees |
| --- | --- |
| `Window::new(id, title)` | an empty window; `id` is 1 to 32 of `[a-z0-9-]` |
| `.text(key, values)` | a line |
| `.row(Row::new(key, values).button(label, action, values))` | a line with up to 4 buttons |
| `.action(id)` | the action whole: its fields above a Run button, or a list's players with a switch each |

Texts are keys of the mod's texts, filled with the values in each player's
language. A button runs its action with the values it gives. When the
action takes more, a press opens a form with fields for the rest, such as a
ban's reason after the row's player.

`actions.show(window, &players)` shows the window to the players with those
account keys. Shown again to a player under the same id, it updates in
place. Each player keeps the window shown to them, so two players may hold
different windows under one id.
`actions.close(id, &players)` closes it on their clients. A mod keeps the
newest 1,024 windows, counted per player, each up to 64 items. A player
whose window went past that sees it close at their next update.

| Call | Where the window shows after it |
| --- | --- |
| `actions.show(window, &players)` | on every client named: it opens, or updates where it shows |
| `actions.update(window, &players)` | only on the clients named that still show it; it opens for no one |
| `actions.close(id, &players)` | on none of the clients named |

Use `update` for a list that changes while players hold it, such as a
roster when a player joins. A player who closed the window does not get it
back. When a client closes a window, its player leaves, or the window
renders nothing for that player, the mod forgets that window for the player. `actions.on_closed(fn)` tells the mod of it,
with the player's account key and the window's id, or no id for a player
who left:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.on_closed(|player, window| match window {
        Some(id) => enshwick::info!("{player:?} closed {id}"),
        None => enshwick::info!("{player:?} left"),
    });
    Ok(())
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:on_closed(function(player, window)
    enshwick.log.info(`{player} closed {window or "every window: they left"}`)
end)
```

A window carries at most 7,168 bytes
to one player. When it does not fit, the players its actions' arguments
offer go first, and the player names one by typing. Then items past the
bound are left out. When the actions it names fill
that alone, it closes, and the server's log names the window.

```rust
use enshwick::actions::{Action, Arg, Outcome, Row, Value, Window};
use enshwick::i18n::Texts;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(Texts::default())?;
    actions.add(
        Action::new("ban", "Ban")
            .admins()
            .arg(Arg::player("Player"))
            .arg(Arg::text("Reason", 120)),
        |_| Outcome::done("Banned"),
    )?;
    let shown = actions.clone();
    actions.add(Action::new("panel", "Admin panel").admins(), move |request| {
        let ann = Value::Player { key: enshwick::PlayerKey::new(7), name: "Ann".into() };
        let window = Window::new("panel", "Admin panel")
            .text("Players", &[])
            .row(Row::new("Ann", &[]).button("Ban", "ban", &[ann]));
        match shown.show(window, &[request.player.key]) {
            Ok(()) => Outcome::done(""),
            Err(why) => Outcome::refused(why.to_string()),
        }
    })?;
    actions.control("panel", enshwick::controls::Key::parse("F7")?)
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({
    id = "ban",
    label = "Ban",
    admins = true,
    args = { enshwick.actions.player("Player"), enshwick.actions.text("Reason", 120) },
}, function() return enshwick.actions.done("Banned") end)
actions:add({ id = "panel", label = "Admin panel", admins = true }, function(request)
    actions:show({
        id = "panel",
        title = "Admin panel",
        items = {
            { Text = { key = "Players" } },
            { Row = { text = { key = "Ann" }, buttons = {
                { label = "Ban", action = "ban", args = { { Player = { key = "7", name = "Ann" } } } },
            } } },
        },
    }, { request:player().key })
    return enshwick.actions.done("")
end)
actions:control("panel", "F7")
```

The server decides what each player sees:

- After its step, the server renders the window for each player in their
  language. A player gets it only when they may run every action it names;
  a player who may not gets nothing.
- Each run from a window is checked again on the server, as any run.
- After a player runs one of the mod's actions from a window, the server
  renders the mod's windows that player holds again. So a list shows its
  switches as they are now.
- The player closes a window on their client with Close. The newest window
  shows; the one under it shows when it closes. The server renders a closed
  window for that player again only when the mod shows it again, and the
  mod hears of the close.
- A client that loses its server closes every window.

A client without Enshwick gets no window and plays on. A dedicated server, a
game a player hosts and single player work alike. The host gets its own
windows inside the game. Joined friends get theirs over the network.

## Worked example

[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
declares who may open a Safe as a list action:

- `.at` finds the Safe at hand, the player's own first, through the mod's
  own rules and its extension point.
- `on` names the players allowed there.
- A second action, `list`, names them.

## Testing

With the `test` feature, the [fake loader](testing.md) asks the actions as
the loader does:

- `ask_actions(ActionCall::RUN, ...)` runs one as a control sends it.
- `ask_action_at(&player, language, id, args, target)` runs one on a named
  object.
- `action_targets()` lists the objects a client's runs named.
- `ask_actions(ActionCall::SHEET, &player, language, "", "")` returns the
  mod's sheet, which `actions::sheet::decode` reads.
- `carried_windows(&player, language)` returns the windows that player's
  client holds after the mod's shows, updates and closes, which
  `actions::window::decode` reads.
- `close_window(&player, id)` closes a window as that player's client does,
  and `player_left(key)` makes the player leave as far as windows go. The
  mod hears of both as the loader tells it.
- `console::run("<mod id> <action> ...")` runs the console form. The fake
  hands a line to the mod's own actions unless `console_answer` gave its first
  word an answer.

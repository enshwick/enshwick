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
  console window.

The Mods screen shows a mod and its settings, not its actions.

Actions are the only way players and administrators act in game: the client
keeps typed `/` lines to itself, and single player has no chat.

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
| `.help(key)`                        | a line under the label and in the console's help                                                 |
| `.arg(Arg::...)`                    | an argument, in order; its kinds follow this table                                               |
| `.allow(fn)`, `.admins()`           | who sees the action and may run it                                                               |
| `.at(fn)`                           | what it acts on for a player, or why there is nothing, which the player reads beside it          |

`Arg` makes four kinds of argument:

- `player`
- `choice`
- `number`
- `toggle`

In Luau, `enshwick.actions.actions(texts)` returns the `Actions`. An action
is a table of the builder's fields:

- `id`
- `label`
- `help`
- `args`
- `admins`
- the function `allow(player)`
- the function `at(ask)`
- the function `per_player(ask, subject)`

Only `true` from `allow` lets a player in. With `admins` set, both must
hold. Four functions make the arguments:

- `enshwick.actions.player`
- `choice`
- `number`. It takes the range as `min` and `max`.
- `toggle`

`at` returns `{ Ok = enshwick.actions.subject(id, label) }` on success. On
failure it returns `{ Err = reason }`. `per_player` returns a list of
`{ key, name }`. A subject's `id` is a 64-bit key, so it crosses as its decimal digits.

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

Arguments are typed choices, never free text. Each is one of four kinds:

- a player online, named on the console;
- one of a few options;
- a number in a range with a step;
- on or off.

A list takes the players online and the ones `on` names, so a player
switched on can be switched off while away.

## Handling

The handler runs on the server's game thread, where [world reads](world.md)
work. It gets a `Request` that holds:

- the player;
- the subject `.at` found for this run;
- the values picked, in the order of the arguments (a list hands
  `Value::Player` and `Value::Toggle`);
- the player's language;
- where it came from, `Via::Interface` or `Via::Console`.

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
words. Values are a player by name, a choice by its word, a number, or `on`
or `off`. A list takes a player and `on` or `off`. For example:
`you.trading-post access Ann on`. `<mod id> help` lists the actions with
their words. The mod's id in front keeps two mods' actions apart, see
[how ids are namespaced](concepts/ids.md). The console is an administrator with no character. That is why
`request.player.entity` is 0 and `.admins()` lets it in. It is also why an
`.at` that looks for something near the player finds nothing. Answers are in
English.

## Where it works

Actions run in the game server: the dedicated server, and a client while it
hosts a game, single player included. A control or panel that runs one
needs Enshwick on the player's client. A mod of side `server` has no part on
the client, so it offers its actions through controls that
[run an action](controls.md#a-mod-that-runs-on-the-server-only). The server
sends them to each player's client.

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
- `console::run("<mod id> <action> ...")` runs the console form. The fake
  hands a line to the mod's own actions unless `console_answer` gave its first
  word an answer.

# Player actions

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

A mod declares what players and administrators can do as actions; the
server checks and runs them wherever they come from.

## Where actions run from

A mod that lets players or administrators do something, such as letting a
friend open a chest or picking a reward, declares it once as an action, and
offers it through means of its own:

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
it, `weather help` lists the actions, and a control of the mod runs it with
`actions.run("set", &[Value::Choice(2), Value::Number(10.0)])`.

## Declaring

[`load.actions(texts)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/actions.rs)
returns the mod's `Actions`; call it once,
during `init`, on both sides of a mod of side `both` alike. Texts are keys of
`texts`, the mod's [translations](settings.md), shown in each player's
language; a key without a text shows as it is, so plain English works too.
`actions.add(action, handler)` declares one action:

| Builder                             | What it does                                                                                     |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| `Action::new(id, label)`            | a button; `id` is 1 to 32 of `[a-z0-9-]`, its name on the console                                |
| `Action::per_player(id, label, on)` | a list of players with a switch each; `on` names the players switched on                         |
| `.help(key)`                        | a line under the label and in the console's help                                                 |
| `.arg(Arg::...)`                    | an argument, in order: `player`, `choice`, `number`, `toggle`                                    |
| `.allow(fn)`, `.admins()`           | who sees the action and may run it                                                               |
| `.at(fn)`                           | what it acts on for a player, or why there is nothing, which the player reads beside it          |

In Luau, `enshwick.actions.actions(texts)` returns the `Actions`, and an
action is a table of the builder's fields: `id`, `label`, `help`, `args`,
`admins`, and the functions `allow(player)`, `at(ask)` and
`per_player(ask, subject)`. Only `true` from `allow` lets a player in, and
with `admins` both must hold. `enshwick.actions.player`, `choice`, `number`
and `toggle` make the arguments; `number` takes the range as `min` and
`max`. `at` returns `{ Ok = enshwick.actions.subject(id, label) }` or
`{ Err = reason }`, and `per_player` returns a list of `{ key, name }`. A
subject's `id` is a 64-bit key, so it crosses as its decimal digits.

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

Arguments are typed choices, never free text: a player online, named on
the console; one of a few options; a number in a range with a step; on or
off. A list takes the players online and the ones `on` names, so a player
switched on can be switched off while away.

## Handling

The handler runs on the server's game thread, where [world reads](world.md)
work, with a `Request`: the player, the subject `.at` found for this run,
the values picked in the order of the arguments (a list hands
`Value::Player` and `Value::Toggle`), the player's language and where it
came from, `Via::Interface` or `Via::Console`. `request.say(key, values)`
fills the mod's text in the player's language. It returns an `Outcome`,
`Outcome::done(text)` or `Outcome::refused(text)`: from a control or panel
the text shows as one of the game's notices, its lines joined into one and
cut to a notice's length, or as a chat line to a client that takes no
notices; from the console it is the console's answer.

A Luau handler gets the request as an object whose methods read those
fields: `request:player()`, `subject()`, `args()`, `language()`, `via()`
and `say(key, values)`, with `values` a list of `{ name, value }`. Each
value is a table of one variant, such as `{ Number = 10 }`, and a choice is
its index from 0, as in Rust. The handler returns
`enshwick.actions.done(text)` or `enshwick.actions.refused(text)`; nil is
done without a text. A Luau client runs an action with
`actions:run(id, values)`.

The server decides everything. It checks `.allow` and runs `.at` again on
every run, whatever the client sent, and refuses values outside their
types and ranges before the handler sees them. A mod's own permission rules,
[extension points](extending.md) included, belong in `.at`, so the
mod's controls and panels and the console all follow them.

The platform's replies, such as "No player named Bob.", are under
`actions.*` keys and take the mod's texts over them.

## From the console

A console line names the mod by its id, then the action and its values as
words: a player by name, a choice by its word, a number, `on` or `off`. A
list takes a player and `on` or `off`: `you.trading-post access Ann on`. `<mod id> help`
lists the actions with their words. The console is an administrator with no
character, so `request.player.entity` is 0, `.admins()` lets it in, and an
`.at` that looks for something near the player finds nothing. Answers are in
English.

## Where it works

Actions run in the game server: the dedicated server, and a client while it
hosts a game, single player included. A control or panel that runs one
needs Enshwick on the player's client. A mod of side `server` has no part on
the client to declare controls or panels, so its actions run only from the
console.

## Worked example

[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
declares who may open a Safe as a list action: `.at` finds the
Safe at hand, the player's own first, through the mod's own rules and its
extension point; `on` names the players allowed there; a second action,
`list`, names them.

## Testing

With the `test` feature, the [fake loader](testing.md) asks the actions as
the loader does: `ask_actions(ActionCall::RUN, ...)` runs one as a control
sends it, `ask_actions(ActionCall::SHEET, &player, language, "", "")`
returns the mod's sheet, which `actions::sheet::decode` reads, and
`console::run("<mod id> <action> ...")` runs the console form: the fake
hands a line to the mod's own actions unless `console_answer` gave its first
word an answer.

# Player actions

A mod that lets players or administrators do something, such as letting a
friend open a chest or picking a reward, declares it once as an action, and
the platform offers it in the game's own interface:

- on the mod's page of the [Mods screen](interface.md#the-mods-screen), in
  the game's settings rows and buttons;
- for an action with a hint, as a line on the HUD while what it acts on is
  at hand, such as "[F7] Who may use it: your Safe at (2, 10, -4)", with a
  key that opens a window of the mod's actions right there;
- on the server's [console](chat.md#the-console), as the mod's id, the
  action and its values, such as a line in a dedicated server's command file.

Actions are the only way players and administrators act in game: the client
keeps typed `/` lines to itself, and single player has no chat.

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let texts = load.texts("[en]\n\"set\" = \"Change the weather\"\n")?;
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

An administrator now finds "Change the weather" under Actions on the Mods
screen, with a dropdown and a slider. With the mod's id `weather`, the
console line `weather set storm 10` runs it too, and `weather help` lists
the actions.

## Declaring

[`load.actions(texts)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/actions.rs)
returns the mod's `Actions`; call it once,
during `init`, on both sides of a join mod alike. Texts are keys of
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
| `.hint()`                           | a HUD line and the key while the subject is at hand; needs `.at`                                 |

Arguments are typed choices, never free text: a player online, picked from
a list and named on the console; one of a few options; a number in a range
with a step; on or off. A list shows the players online and the ones `on`
names, so a player switched on can be switched off while away.

## Handling

The handler runs on the server's game thread, where [world reads](world.md)
work, with a `Request`: the player, the subject `.at` found for this run,
the values picked in the order of the arguments (a list hands
`Value::Player` and `Value::Toggle`), the player's language and where it
came from, `Via::Interface` or `Via::Console`. `request.say(key, values)`
fills the mod's text in the player's language. It returns an `Outcome`,
`Outcome::done(text)` or `Outcome::refused(text)`: from the interface the
text shows as one of the game's notices and on the screen that sent it; from
the console it is the console's answer.

The server decides everything. It checks `.allow` and runs `.at` again on
every run, whatever the client showed, and refuses values outside their
types and ranges before the handler sees them. A mod's own permission rules,
[extension points](extending.md) included, belong in `.at`, so the
interface, the hint and the console all follow them.

The platform's replies, such as "No player named Bob.", are under
`actions.*` keys and take the mod's texts over them.

## From the console

A console line names the mod by its id, then the action and its values as
words: a player by name, a choice by its word, a number, `on` or `off`. A
list takes a player and `on` or `off`: `safes access Ann on`. `<mod id> help`
lists the actions with their words. The console is an administrator with no
character, so `request.player.entity` is 0, `.admins()` lets it in, and an
`.at` that looks for something near the player finds nothing. Answers are in
English.

## Where it works

Actions run in the game server: the dedicated server, and a client while it
hosts a game, single player included. The interface needs Enshwick on the
player's client. The window and
the Mods screen page ask the server each time they open, and the server
sends the hints a player has at hand once a second while they change. The
key is `F7`, `actions_key` in the loader's settings, read only while the
game's window has the focus and the HUD shows.

## Worked example

[`enshwick-safes`](https://github.com/enshwick/enshwick/tree/main/mods/enshwick-safes)
declares who may open a Safe as a list action with a hint: `.at` finds the
Safe at hand, the player's own first, through the mod's own rules and its
extension point; `on` names the players allowed there; a second action,
`list`, names them.

## Testing

With the `test` feature, the [fake loader](testing.md) asks the actions as
the loader does: `ask_actions(ActionCall::SHEET, &player, language, "", "")`
returns the sheet, which `actions::sheet::decode` reads,
`ask_actions(ActionCall::RUN, ...)` runs one from the interface, and
`console::run("<mod id> <action> ...")` runs the console form: the fake
hands a line to the mod's own actions unless `console_answer` gave its first
word an answer.

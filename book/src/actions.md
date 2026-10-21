# Player actions

Players forget chat commands. A mod that lets players do something, such as
letting a friend open a chest or picking a reward, declares it once as an
action, and the platform offers it in the game's own interface, where
players find it, and as a chat command, for those who type:

- on the mod's page of the [Mods screen](interface.md#the-mods-screen), in
  the game's settings rows and buttons;
- for an action with a hint, as a line on the HUD while what it acts on is
  at hand, such as "[F7] Who may use it: your Safe at (2, 10, -4)", with a
  key that opens a window of the mod's actions right there;
- as `/<command> <action> ...` in chat, with help made from the
  declarations.

Declaring an action costs about as much as registering a command, and it is
the recommended way for anything a player does. Keep plain
[commands](commands.md) for administrators' and developers' tools.

```rust
use enshwick_loader_api::actions::{Action, Arg, Outcome, Value};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let texts = load.texts("[en]\n\"set\" = \"Change the weather\"\n")?;
    let actions = load.actions("weather", texts)?;
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
screen, with a dropdown and a slider, and may type
`/weather set storm 10`. `/weather help` lists every action the player may
use.

## Declaring

[`load.actions(command, texts)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/actions.rs)
registers the chat command and returns the mod's `Actions`; call it once,
during `init`, on both sides of a join mod alike. Texts are keys of
`texts`, the mod's [translations](settings.md), shown in each player's
language; a key without a text shows as it is, so plain English works too.
`actions.add(action, handler)` declares one action:

| Builder                             | What it does                                                                                     |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| `Action::new(id, label)`            | a button; `id` is 1 to 32 of `[a-z0-9-]`, typed after the command                                |
| `Action::per_player(id, label, on)` | a list of players with a switch each; `on` names the players switched on                         |
| `.help(key)`                        | a line under the label and in the chat help                                                      |
| `.arg(Arg::...)`                    | an argument, in order: `player`, `choice`, `number`, `toggle`                                    |
| `.allow(fn)`, `.admins()`           | who sees the action and may run it                                                               |
| `.at(fn)`                           | what it acts on for a player, or why there is nothing, which the player reads beside it          |
| `.hint()`                           | a HUD line and the key while the subject is at hand; needs `.at`                                 |
| `.verbs(on, off)`                   | two chat words for a list: `/safe allow <player>`, `/safe deny <player>`                         |

Arguments are typed choices, never free text: a player online, picked from
a list and typed by name in chat; one of a few options; a number in a range
with a step; on or off. A list shows the players online and the ones `on`
names, so a player switched on can be switched off while away.

## Handling

The handler runs on the server's game thread, where [world reads](world.md)
work, with a `Request`: the player, the subject `.at` found for this run,
the values picked in the order of the arguments (a list hands
`Value::Player` and `Value::Toggle`), the player's language and where it
came from, `Via::Interface` or `Via::Chat`. `request.say(key, values)` fills
the mod's text in the player's language. It returns an `Outcome`,
`Outcome::done(text)` or `Outcome::refused(text)`: from the interface the
text shows as one of the game's notices and on the screen that sent it; from
chat it goes to the player's chat.

The server decides everything. It checks `.allow` and runs `.at` again on
every run, whatever the client showed, and refuses values outside their
types and ranges before the handler sees them. A mod's own permission rules,
[extension points](extending.md) included, belong in `.at`, so the
interface, the hint and the command all follow them.

`actions.set_language(language)` sets the language of the chat form; the
interface uses each player's own. The platform's replies, such as "No player
named Bob.", are under `actions.*` keys and take the mod's texts over them.

## Where it works

Actions run in the game server: the dedicated server, and a client while it
hosts a game, single player included. The interface needs Enshwick on the
player's client; a player without it still has the command. The window and
the Mods screen page ask the server each time they open, and the server
sends the hints a player has at hand once a second while they change. The
key is `F7`, `actions_key` in the loader's settings, read only while the
game's window has the focus and the HUD shows.

## Testing

With the `test` feature, the [fake loader](testing.md) asks the actions as
the loader does: `ask_actions(ActionCall::SHEET, &player, language, "", "")`
returns the sheet, which `actions::sheet::decode` reads,
`ask_actions(ActionCall::RUN, ...)` runs one from the interface, and
`command(name, &player, args)` runs the chat form.

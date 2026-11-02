# Chat, notices and the console

A mod sends players chat lines and notices on screen. A dedicated server's
console runs the lines typed into its window or appended to its command file.

## No commands for players

Players never type commands: the game client keeps every chat line that
starts with `/` to itself, and single player has no chat. Anything a player
or an administrator does is a [player action](actions.md), shown in the
game's interface.

## Chat lines

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

Chat lines work on the dedicated server and on a client while it hosts a
game.

[`world.broadcast(text)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world.rs)
sends a line to every player and
[`world.tell(&player, text)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world.rs)
to one player. Call them from any thread: the line waits in a queue, and the
server posts it after the next step. Each line of `text` becomes a message of
its own, and a message keeps at most 16 lines. The queue holds at most 256
lines; more are dropped.

## Notices on screen

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

[`world.notify(&player, text)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world.rs)
shows a line on the player's screen, as the game shows its own "can't do
that" messages, and `world.notify_all(text)` on every player's. It returns
how the line went: `Notice::OnScreen`, or `Notice::Chat` when the player's
client lacks Enshwick and gets the line in chat instead. Call it on the game
thread. The game shows up to 255 bytes; keep it to a short line.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::world::Notice;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("ping-me", "Show me a notice"), move |request| {
        match world.notify(request.player, "Pong") {
            Notice::OnScreen => Outcome::done("On screen"),
            _ => Outcome::done("In chat"),
        }
    })
}
```

The game raises these messages on the client, in the client's own world, so
the server cannot raise one for a client. A notice travels as a
[message](messages.md) to the client's loader, which shows the text through a
text tag of its own and raises the game's message event in the client's world
at its next step.

## The console

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

The console runs a line on the server as an administrator with no character.
A line is one of:

- `enshwick ...`, the loader's own tools, listed below;
- a mod's id and the text form of one of its [actions](actions.md#from-the-console):
  `you.trading-post help`, `you.my-mod give-to 3073 10 Ann`.

[`console::run(line)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/console.rs)
runs a line and returns its answer. Call it on the server's game thread, from
a scheduled task or a game event; elsewhere it returns
`RunError::NotOnGameThread`. A first word that neither the loader nor a
running mod with actions answers returns `RunError::Unknown`.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let scheduler = load.scheduler()?;
    scheduler.next_tick(|| match enshwick::console::run("enshwick status") {
        Ok(answer) => enshwick::info!("{answer}"),
        Err(why) => enshwick::warn!("not run: {why}"),
    });
    Ok(())
}
```

The answer goes back to the caller, not to any chat.

### The command file

A dedicated server runs each line appended to
`mod-data/enshwick/commands.txt` on its console, once, about a second after
the file is saved. Only complete lines run, so a line waits for its line
break; blank lines and lines starting with `#` are skipped, and so are the
lines already in the file when the server starts. Each line and its answer
go to `logs/enshwick/loader.log`.

The file is append-only. When lines that already ran are changed or
removed, no line runs again: the loader counts the lines that ran, runs only
the lines past that count, and writes a warning to the log. To start over,
empty the file; its next lines run from the first. A file that another
program holds open is read again a second later, and no line runs twice. A
file that is deleted and made again runs from its first line, unless it
starts with the lines that already ran. Administrators and their scripts reach the
console this way without anyone joining.

### The server's console window

A line typed into a dedicated server's own console window, or its terminal,
runs the same way, and its answer shows there and in the log. In Docker the
container needs input: `docker run -it`, or `stdin_open: true` and
`tty: true` in Compose, then `docker attach` to type. A server started without
input, such as a Windows service, reads only its command file.

### The loader's tools

| Line                                            | Answer                                                                                                                                                                                                                        |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `enshwick`                                      | the loader version and every mod with its name and version, `(off)` for mods turned off                                                                                                                                       |
| `enshwick <mod id>`                             | what the mod is: name, version, description, authors, licence, homepage, and notes such as a recommended mod that is missing                                                                                                  |
| `enshwick help`                                 | the tools, and the mods whose actions the console runs                                                                                                                                                                        |
| `enshwick mods`                                 | each mod with its side, whether players need it to join, and whether it runs                                                                                                                                                  |
| `enshwick world`                                | the players online with their account, slot, character entity and place                                                                                                                                                       |
| `enshwick entity <number>`                      | what the server knows of an entity: template, the item it was placed from, place, whose character it is                                                                                                                       |
| `enshwick placed [n]`                           | the pieces players placed last, newest first, at most 32                                                                                                                                                                      |
| `enshwick reload <mod id>`                      | makes the mod read its settings file again, the running world's own copy when it has one                                                                                                                                      |
| `enshwick kick <player>`                        | asks the game to remove a player online                                                                                                                                                                                       |
| `enshwick ban <player>`                         | bans a player or an account, removing the player when online; the ban stays in `bannedAccounts` of `enshrouded_server.json`                                                                                                   |
| `enshwick unban <account>`                      | asks the game to take an account off its ban list                                                                                                                                                                             |
| `enshwick allowlist`                            | whether the [allowlist](world.md#the-allowlist) is on, and how many accounts it lists                                                                                                                                         |
| `enshwick allowlist add <player>`               | lists a player or an account on the allowlist, with the player's name as its note                                                                                                                                             |
| `enshwick allowlist remove <player>`            | takes a player or an account off the allowlist                                                                                                                                                                                |
| `enshwick give <item> <count>`                  | gives every player online `count` new items, at most 10,000, each into the player's own inventory, with the game's pickup message; `enshwick recipes find <text>` shows item ids                                              |
| `enshwick give-to <item> <count> <player>`      | the same for one player online                                                                                                                                                                                                |
| `enshwick spawn <template> <count> <x> <y> <z>` | spawns `count` entities from a template GUID, as `enshwick entity` shows it, at a place in metres, 2 m apart, at most 32; with a player instead of the place, beside that player; the game drops them for an unknown template |
| `enshwick teleport <x> <y> <z> <player>`        | moves a player online to a place in metres, the place `enshwick world` shows                                                                                                                                                  |
| `enshwick status`                               | a short [status summary](logging.md#status-summary), and a fresh `status.log`                                                                                                                                                 |
| `enshwick profile [reset]`                      | the five [costliest mods](logging.md#per-mod-costs) since start, or since `enshwick profile reset`                                                                                                                            |
| `enshwick conflicts`                            | the hooks, filters and resources several mods [share](logging.md#conflicts)                                                                                                                                                   |
| `enshwick log [mod] [level] [n]`                | the latest [log lines](logging.md#reading-the-log-from-the-console), of one mod or all, at a level or more severe                                                                                                             |
| `enshwick recipes [find <text>]`                | the [disabled recipes](content/disabled-recipes.md#finding-names-and-changing-rules-from-the-console), or recipes found by name; `disable` and `enable` change the rules                                                      |
| `enshwick config [mod] [key] [value]`           | read and change a mod's settings; see [The schema and the console](settings.md#the-schema-and-the-console)                                                                                                                    |
| `enshwick ui-demo <player>`                     | opens a window of every [interface](interface.md) widget on that player's client, when it runs Enshwick, for a visual check                                                                                                   |

A player is named by name, or by account: the 16 hex digits
`enshwick world` shows, or hex with `0x` in front. A name several players
share is refused, with their accounts. `kick` and `ban` follow the game's
rules, such as for administrators and accounts not online; see
[Kicking and banning](world.md#kicking-and-banning).

Places for `spawn` and `teleport` lie within 100 km of the origin on each
axis; a place farther out gets the usage line. In a game a client hosts,
`give` works only while a mod has asked for inventories.

No mod may take the id `enshwick`.

## Worked examples

- [`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
  declares who may open a Safe as [player actions](actions.md).

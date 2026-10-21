# Commands and chat

A mod can answer chat commands: a player types `/name words...`, and the
server hands the line to the mod that registered `name`. The line never
reaches anyone's chat, and the reply goes to that player alone.

Commands, chat lines, labels and rename filters work on the dedicated server
and on a client while it hosts a game.

## Registering a command

[`load.add_command(name, help, handler)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs)
registers `/name` with a line of help:

```rust
use enshwick_loader_api::commands::Reply;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.add_command("safe", "allow, deny or list players at your nearest Safe", |ctx, args| {
        match args {
            ["list"] => match &ctx.player {
                Some(player) => Reply::from(format!("{} asked for the list", player.name)),
                None => Reply::from("the console has no Safes"),
            },
            _ => Reply::from("/safe allow <player>, /safe deny <player>, /safe list"),
        }
    })
}
```

- Register during `init`; the loader refuses commands after it.
- A name is 1 to 32 characters of `[a-z0-9-]`
  ([`commands::is_valid_name`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs)).
  A malformed name, `enshwick`, or a name another mod holds is refused:
  `load.add_command` returns `Err(Off)`.
- The player may type the name in any case: `/Safe` runs `/safe`.

## The handler

The loader runs the handler on the server's game thread, where
[world reads](world.md) work, with two arguments:

- a [`CommandContext`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs):
  `ctx.player`, the [`Player`](world.md#players) who typed it (`None` for the
  server's console), `ctx.character()`, that player's character, `ctx.name`,
  the command's name, and `ctx.is_admin()`, true for the console too;
- the words after the name, split at whitespace, as `&[&str]`.

The handler returns a
[`Reply`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs),
built `From` a `String` or `&str`. Each line of it becomes a chat message to
that player; empty lines are dropped and a reply keeps at most 16 lines.
`Reply::none()` sends nothing.

Check permissions in the handler: every player can type every command.

```rust
use enshwick_loader_api::commands::{CommandContext, Reply};

fn kick_all(ctx: &CommandContext, _args: &[&str]) -> Reply {
    if !ctx.is_admin() {
        return Reply::from("Only administrators may do that.");
    }
    Reply::none()
}
```

A panic in the handler turns the mod off and sends no reply. A command of a
mod that is off answers as unknown.

## Built-in commands

The loader answers `/enshwick` itself:

| Command                           | Answer                                                                                                                                             |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/enshwick`                       | the loader version and every mod with its name and version, `(off)` for mods turned off                                                            |
| `/enshwick <mod id>`              | what the mod is: name, version, description, authors, licence, homepage, and notes such as a recommended mod that is missing                       |
| `/enshwick help`                  | every command of a running mod, with its mod id and help line                                                                                      |
| `/enshwick status`                | administrators only: a short [status summary](logging.md#status-summary), and a fresh `status.log`                                                 |
| `/enshwick profile`               | administrators only: the five [costliest mods](logging.md#per-mod-costs) since start, or since `/enshwick profile reset`                           |
| `/enshwick conflicts`             | administrators only: the hooks, filters and resources several mods [share](logging.md#conflicts)                                                   |
| `/enshwick log [mod] [level] [n]` | administrators only: the latest [log lines](logging.md#reading-the-log-in-game), of one mod or all, at a level or more severe                      |
| `/enshwick ui-demo`               | administrators only, from a client that runs Enshwick: opens a window of every [interface](interface.md) widget on that client, for a visual check |
| `/config <mod> [key] [value]`     | administrators only: read and change a mod's settings; see [The schema and `/config`](settings.md#the-schema-and-config)                           |

No mod may register `enshwick`, `config` or `enshwick-net`, which carries the
loader's own messages between client and server. A `/` command no one has gets
`Unknown command /name. /enshwick help lists the commands.`

## Running a command line

[`commands::run(line)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs)
runs a command line as the server's console: an administrator without a
character, whose handlers get `ctx.player` as `None`. Any mod's command and
the loader's own work, and it returns the reply.

Call it on the server's game thread, from a scheduled task or a game event.
Elsewhere it returns `RunError::NotOnGameThread`.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let scheduler = load.scheduler()?;
    scheduler.next_tick(|| match enshwick_loader_api::commands::run("/enshwick status") {
        Ok(reply) => enshwick_loader_api::info!("{}", reply.text()),
        Err(why) => enshwick_loader_api::warn!("not run: {why}"),
    });
    Ok(())
}
```

A reply to the console is returned to the caller, not sent.

## Chat lines

[`world.broadcast(text)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/world.rs)
sends a line to every player and
[`world.tell(&player, text)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/world.rs)
to one player. Call them from any thread: the line waits in a queue, and the
server posts it after the next step. Each line of `text` becomes a message of
its own. The queue holds at most 256 lines; more are dropped.

## Notices on screen

[`world.notify(&player, text)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/world.rs)
shows a line on the player's screen, as the game shows its own "can't do
that" messages, and `world.notify_all(text)` on every player's. It returns
how the line went: `Notice::OnScreen`, or `Notice::Chat` when the player's
client lacks Enshwick and gets the line in chat instead. Call it on the game
thread; it needs ABI minor 27, and under an older loader it always tells in
chat. The game shows up to 255 bytes; keep it to a short line.

```rust
use enshwick_loader_api::world::Notice;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    load.add_command("ping-me", "shows a notice", move |ctx, _args| {
        let Some(player) = &ctx.player else {
            return "the console sees no notices".into();
        };
        match world.notify(player, "Pong") {
            Notice::OnScreen => "on screen".into(),
            _ => "in chat".into(),
        }
    })
}
```

The game raises these messages on the client, in the client's own world, so
the server cannot raise one for a client. A notice travels as a
[message](messages.md) to the client's loader, which shows the text through a
text tag of its own and raises the game's message event in the client's world
at its next step.

## Worked examples

- [`enshwick-admin-tools`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-admin-tools)
  registers `/admin` for administrators, answers with `broadcast` and `tell`,
  and runs each line appended to `mod-data/admin-tools/commands.txt` with
  `commands::run` from a scheduled task.
- [`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
  registers `/safe` to allow and deny players at a Safe, and guards its labels
  with a rename filter.

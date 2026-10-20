# Commands and chat

A mod can answer chat commands: a player types `/name words...`, and the
server hands the line to the mod that registered `name`. The line never
reaches anyone's chat, and the reply goes to that player alone.

Commands, chat lines, labels and rename filters work on the dedicated server
and on a client while it hosts a game.

## Registering a command

[`load.command(name, help, handler)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/commands.rs)
registers `/name` with a line of help:

```rust
use enshwick_api::commands::Reply;

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    load.command("safe", "allow, deny or list players at your nearest Safe", |ctx, args| {
        match args {
            ["list"] => Reply::from(format!("{} asked for the list", ctx.player.name)),
            _ => Reply::from("/safe allow <player>, /safe deny <player>, /safe list"),
        }
    })
}
```

- Register during `init`; the loader refuses commands after it.
- A name is 1 to 32 characters of `[a-z0-9-]`
  ([`commands::is_valid_name`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/commands.rs)).
  A malformed name, `enshwick`, or a name another mod holds is refused:
  `load.command` returns `Err(Off)`.
- The player may type the name in any case: `/Safe` runs `/safe`.

## The handler

The loader runs the handler on the server's game thread, where
[world reads](world.md) work, with two arguments:

- a [`CommandContext`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/commands.rs):
  `ctx.player`, the [`Player`](world.md#players) who typed it, `ctx.name`, the
  command's name, and `ctx.is_admin()`;
- the words after the name, split at whitespace, as `&[&str]`.

The handler returns a
[`Reply`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/commands.rs),
built `From` a `String` or `&str`. Each line of it becomes a chat message to
that player; empty lines are dropped and a reply keeps at most 16 lines.
`Reply::none()` sends nothing.

Check permissions in the handler: every player can type every command.

```rust
use enshwick_api::commands::{CommandContext, Reply};

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

| Command | Answer |
| --- | --- |
| `/enshwick` | the loader version and every mod with its version, `(off)` for mods turned off |
| `/enshwick help` | every command of a running mod, with its mod id and help line |
| `/enshwick status` | administrators only: a short [status summary](logging.md#status-summary), and a fresh `status.log` |

A `/` command no one has gets `Unknown command /name. /enshwick help lists
the commands.`

## Running a command line

[`commands::run(line)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/commands.rs)
runs a command line as the server's console: an administrator named `console`
with no character (entity 0, account key 0). Any mod's command and the
loader's own work, and it returns the reply.

Call it on the server's game thread, from a scheduled task or a game event.
Elsewhere it returns `RunError::NotOnGameThread`.

```rust
fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let scheduler = load.scheduler()?;
    scheduler.next_tick(|| match enshwick_api::commands::run("/enshwick status") {
        Ok(reply) => enshwick_api::info!("{}", reply.text()),
        Err(why) => enshwick_api::warn!("not run: {why}"),
    });
    Ok(())
}
```

A handler that gets the console has a `Player` with `key` 0: a reply to it is
returned, not sent, and `world.tell` to it sends nothing.

## Chat lines

[`world.broadcast(text)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/world.rs)
sends a line to every player and
[`world.tell(&player, text)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/world.rs)
to one player. Call them from any thread: the line waits in a queue, and the
server posts it after the next step. Each line of `text` becomes a message of
its own. The queue holds at most 256 lines; more are dropped.

## Worked examples

- [`enshwick-dev-tools`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-dev-tools)
  registers `/dev` for administrators, answers with `broadcast` and `tell`,
  and runs each line appended to `mod-data/dev-tools/commands.txt` with
  `commands::run` from a scheduled task.
- [`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
  registers `/safe` to allow and deny players at a Safe, and guards its labels
  with a rename filter.

# Chat, notices and the console

A mod sends players chat lines and notices on screen. A dedicated server's
console runs the lines typed into its window.

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

## Hearing and filtering players' lines

On the server, a mod hears every line that reaches the chat, and may drop or
rewrite what players type before anyone reads it.

`load.on_chat_said(handler)` calls `handler` after the step with a
`ChatLine`: the sender's account and character, its kind (`Player`, `Emote`
or `Notice`, such as a player joining) and its text. A player's line arrives
as the filters left it, at most eight a second per player; lines that mods
send never arrive.

`load.on_chat(filter)` runs `filter` on each line a player types, inside the
game's chat post, after the chat mute. It answers `ChatVerdict::Keep`,
`Drop` or `Rewrite(text)`. Filters of all mods run in load order: a drop
stops the filters after it, and a rewrite hands its text on. A muted
player's line reaches no filter and no handler. A filter that panics turns
off alone, and lines pass it.

```rust
use enshwick::chat::ChatVerdict;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_chat(|line| {
        if line.text.contains("spoiler") {
            ChatVerdict::Rewrite("[hidden]".to_owned())
        } else {
            ChatVerdict::Keep
        }
    })?;
    load.on_chat_said(|line| enshwick::info!("{:?} line of {} bytes", line.kind, line.text.len()))
}
```

From Luau, the same calls are `enshwick.chat.on_chat(fn)` and
`enshwick.chat.on_chat_said(fn)`; a Luau filter returns `false` to drop the
line, a string to rewrite it, and nil or `true` to keep it. An empty
rewrite drops the line too.

```luau
enshwick.chat.on_chat(function(line)
    if line.text:find("spoiler") then
        return "[hidden]"
    end
    return nil
end)
enshwick.chat.on_chat_said(function(line)
    enshwick.log.info(`{line.kind} line of {#line.text} bytes`)
end)
```

Every server mod reads the full text of every line. The loader logs only
counts and lengths. A mod that stores what players type says so on its
player page.

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

A Luau mod calls the same functions of `enshwick.world`; here each player who
joins gets a notice:

```luau
enshwick.on("PlayerJoined", function(joined)
    local player = enshwick.world.player(joined.entity)
    if player and enshwick.world.notify(player, "Welcome") == "Chat" then
        enshwick.log.info(`{player.name} reads notices in chat`)
    end
end)
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

### The server's console window

A line typed into a dedicated server's own console window, or its terminal,
runs the same way, and its answer shows there and in the loader's log. In
Docker the container needs input: `docker run -it`, or `stdin_open: true`
and `tty: true` in Compose, then `docker attach` to type. A server started
without input, such as a Windows service, runs only the lines mods run.

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
| `enshwick status`                               | a short [status summary](logging.md#status-summary), and the whole summary in the loader's log                                                                                                                                               |
| `enshwick profile [reset]`                      | the five [costliest mods](logging.md#per-mod-costs) since start, or since `enshwick profile reset`                                                                                                                            |
| `enshwick conflicts`                            | the hooks, filters and resources several mods [share](logging.md#conflicts)                                                                                                                                                   |
| `enshwick log [mod] [level] [n]`                | the latest [log lines](logging.md#reading-the-log-from-the-console), of one mod or all, at a level or more severe                                                                                                             |
| `enshwick recipes [find <text>]`                | the [disabled recipes](content/disabled-recipes.md#finding-names-and-changing-rules-from-the-console), or recipes found by name; `disable` and `enable` change the rules                                                      |
| `enshwick config [mod] [key] [value]`           | read and change a mod's settings; see [The schema and the console](settings.md#the-schema-and-the-console)                                                                                                                    |
| `enshwick ui-demo <player>`                     | opens a window of every [interface](interface.md) widget on that player's client, when it runs Enshwick, for a visual check                                                                                                   |

The loader has no tools that kick, ban, give items, spawn or move players:
those are a mod's, through the [account actions](world.md#kicking-and-banning)
and the rest of the API.

No mod may take the id `enshwick`.

## Worked examples

- [`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
  declares who may open a Safe as [player actions](actions.md).

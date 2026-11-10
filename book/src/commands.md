<!--
type: how-to
description: A mod names its player actions as commands, which run from chat, the server's console and a remote console with the caller's rights.
-->

# Commands

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

A mod names its [actions](actions.md) as commands, and players,
administrators and remote tools then type them.

## Name the actions as commands

Call `Actions::commands` once during `init`, with a short alias. The
command's name is the part of the mod's id after the author: the mod
`you.admin-tools` gets `/admin-tools`. The alias is 1 to 32 lowercase
letters, digits and hyphens, shorter than the name, and neither `enshwick`
nor `help`.

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::i18n::Texts;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(Texts::default())?;
    actions.add(Action::new("online", "Players online"), |_| Outcome::done("2 online"))?;
    actions.add(
        Action::new("kick", "Kick a player")
            .group("Players")
            .help("Removes the player from the server.")
            .admins()
            .arg(Arg::player("Player")),
        |request| match request.args {
            [Value::Player { name, .. }] => Outcome::done(format!("Kicked {name}")),
            _ => Outcome::refused("Name a player"),
        },
    )?;
    // In the mod `you.admin-tools`: `/admin-tools` and `/adm`.
    actions.commands("adm")
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({ id = "online", label = "Players online" }, function()
    return enshwick.actions.done("2 online")
end)
actions:add({
    id = "kick",
    label = "Kick a player",
    group = "Players",
    help = "Removes the player from the server.",
    admins = true,
    args = { enshwick.actions.player("Player") },
}, function(request)
    return enshwick.actions.done(`Kicked {request:args()[1].Player.name}`)
end)
actions:commands("adm")
```

Now `/admin-tools online` and `/adm online` both run the action named online.
The words after the action's id are its values, as on the
[console](actions.md#from-the-console). So `/adm kick Ann` kicks the player
online named Ann. A player the action
[knows while offline](actions.md#players-who-are-offline) is named the same
way. A last text argument takes the rest of the line, as in
`/adm say Restart in five minutes`, and a text before another value takes
one word or a `"quoted run"`. The last arguments of an action may be
[optional](actions.md#optional-arguments).

## Help and mistyped lines

The platform writes every mod's help from the actions the mod declares. A
mod cannot declare an action named `help`.

| The caller types | The reply |
| --- | --- |
| `/adm` or `/adm help` | a line naming the mod, then one line for each action the caller may run: its id, its arguments and its label |
| `/adm help kick` | the line of `kick`, then the text of `Action::help` |
| `/adm kik Ann` | no such action, the closest ids the caller may run, and `/adm help` |
| `/adm kick` | the line of `kick`, since a value is missing |

`Action::group` names the group of an action, a text key. The help lists
the actions without a group first, then each group under its name, in the
order the mod declared them. The first line shows the mod's name as the
Mods screen shows it, and the command as the caller typed it. An optional
argument stands in square brackets. A long help reaches the chat whole while the chat has room: replies to typed
commands take at most half of the 256 lines that wait to be posted.

A command is a second way to an action, never the only one. Players reach
every action through the mod's controls and panels too.

## Who runs a command

The server checks each run as it checks one from the interface. Three
callers type commands:

| Caller | Runs as | The reply goes |
| --- | --- | --- |
| A player, in chat | that player, with their permissions | to that player alone: one line of a done action as a notice, several lines and every refusal in chat |
| The server's console window | the console, with every permission and no character | to the console |
| A remote tool, through another mod | a caller that mod names, with the permissions it gives | back to that mod |

A player's line that starts with `/` and a running mod's command name leaves
the chat. It runs after the step, even while the player is muted in chat.
Any other line posts as it would without the loader. A player runs at most
eight commands a second, and the server drops a flood beyond that.

A handler that panics during a command or a console line turns off only the
mod's typed lines, with a warning in its log. Each later one is refused
with a line saying so. Its actions keep their other paths.

The handler reads where a run came from in `request.via`. Commands typed in
chat arrive as `Via::Chat`, and those another mod runs as `Via::Remote`.

## When two mods want the same alias

An alias goes to the first mod that asks for it. A later mod that asks for
the same word runs by its full name alone: `/admin-tools online` works, and
its alias does not. Nothing turns off. The log and the Mods screen say once
which mod has the alias, and `commands::list` shows the later mod without
one.

A word that is another mod's id or command name is never an alias, whether
or not that mod names commands. So a mod's full name always reaches it.

Two mods of different authors may share a name, such as `you.tools` and
`they.tools`. The first one to name commands keeps `/tools`. The loader
turns off only the later mod's commands, and the log and the Mods screen
name both mods. Its call returns `Ok`, and its actions keep their other
paths.

## Run any mod's command for a remote caller

A mod that serves a remote console, such as an RCON server, runs other
mods' commands without knowing them. It calls `commands::run` with the line
and a `Caller`.

```rust
use enshwick::commands::{self, Caller};
use enshwick::world::PlayerPermissions;

fn on_line(line: &str) -> String {
    let operator = Caller::new("rcon 10.0.0.2").with(PlayerPermissions::CanKickBan);
    match commands::run(line, &operator) {
        Ok(reply) => reply.text().to_owned(),
        Err(why) => format!("not run: {why}"),
    }
}
```

```luau
local reply = enshwick.commands.run("/adm online", "rcon 10.0.0.2", { "CanKickBan" })
enshwick.log.info(reply.text)
```

The caller is never a player. It has no character, its account key is 0,
and the handler reads its label as the player's name. `Caller::with_all`
gives it every permission, as the console holds them.

`commands::run` works on the server's game thread only. A socket thread
calls `commands::run_later` instead. It takes the mod's `Scheduler` and
hands the reply to a callback on the game thread. A line holds at most
1,024 bytes.

`commands::list` names the running mods' command sets in load order, so a
remote console answers `help` across mods.

## Limit an action to a permission

The game's user groups grant each player permissions, set in
`enshrouded_server.json`. A player holds those of the group whose password
they joined with. `Action::needs` limits an action to players holding a
permission. It applies wherever the action runs from.

| `PlayerPermissions` | The group's setting |
| --- | --- |
| `CanKickBan` | `canKickBan`, the server's administrators |
| `CanAccessInventories` | `canAccessInventories` |
| `CanEditBase` | `canEditBase` |
| `CanEditWorld` | `canEditWorld` |
| `CanExtendBase` | `canExtendBase` |
| `CanReceiveExp` | receives experience |

The names come from the game's reflected `PlayerPermissions` on server
26430253. A player without the permission does not see the action. Its
control does not reach their client, and a run is refused.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::world::PlayerPermissions;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let repair = Action::new("repair", "Repair the base").needs(PlayerPermissions::CanEditBase);
    actions.add(repair, |_| Outcome::done("Repaired"))
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({ id = "repair", label = "Repair the base", needs = { "CanEditBase" } }, function()
    return enshwick.actions.done("Repaired")
end)
```

A rule of the mod's own, such as a list of groups in its settings, belongs
in `.allow`. When that rule changes while the world runs, call
`Actions::rules_changed`. The server then carries each player the controls
they may run now, within two seconds.

## Testing

With the `test` feature, the [fake loader](testing.md) takes commands as the
loader does:

- `type_in_chat(&player, language, line)` types a line in chat. It returns
  the mod's reply, or `None` for a line that stays in chat.
- `other_mod_commands(mod_id, alias)` adds a mod loaded before the mod under
  test, and `later_mod_commands(mod_id, alias)` one loaded after it. Each
  names its commands with that alias. A line to it gets the `console_answer`
  of the word used.
- `rules_changed()` counts calls of `Actions::rules_changed`.

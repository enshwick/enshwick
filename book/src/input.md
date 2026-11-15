<!--
type: reference
description: A server mod reads which of the game's controls each player holds this step, a client mod reads its own player's, and what to trust of that.
-->

# The controls a player holds

[`load.input()`](https://github.com/enshwick/enshwick/blob/main/crates/internal/actions/src/input.rs)
returns the
[`Input`](https://github.com/enshwick/enshwick/blob/main/crates/internal/actions/src/input.rs)
of the server's world: which of the game's controls each player holds this
step. It is unstable: it needs the `unstable` feature (see
[Stability](stability.md)).

## Reading what a player holds

`input.held(player)` returns the game actions that player holds this step, as
an `InputMask`. The mask has these calls:

- `contains(GameAction)`;
- `iter()`;
- the raw `bits()`.

It reads the player's own input in the server's world, as their client last
sent it. For that reason it reads a player without Enshwick the same way. It
returns `None` for an entity that is no player.

`Input` is `Copy`. Get it during `init` and move it into ticks and handlers.
`held` runs on the server's game thread between steps, such as in a tick.
Elsewhere it fails with a [`WorldError`](world.md):

- `WrongThread` off the game thread or inside a step;
- `Missing` before a world runs.

```rust
use std::time::Duration;

use enshwick::controls::GameAction;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let input = load.input()?;
    let world = load.world()?;
    load.on_tick(Duration::from_secs(1), move |_| {
        for player in world.players() {
            if let Ok(Some(held)) = input.held(player.entity)
                && held.contains(GameAction::Block)
            {
                enshwick::info!("{} blocks", player.name);
            }
        }
    })?;
    Ok(())
}
```

```luau
enshwick.on_tick(1, function()
    for _, player in enshwick.world.players() do
        local held = enshwick.input.held(player.entity)
        if held ~= nil and table.find(held, "Block") then
            enshwick.log.info(`{player.name} blocks`)
        end
    end
end)
```

In Luau, `held` returns the list of the held actions' game names. They are the
names a [control's](controls.md) `link` uses, such as `"ContextualAction_Tap"`.

## Reading what the local player holds

On a client, `input.local_held()` returns the game actions its own player
holds, as an `InputMask`. It is the mask the client read in its last step.
It returns `None` on a server and outside a game. It also returns `None`
while no step ran for a second, as in the pause menu. The client reads the
mask only after a mod first asks for it, so that first call returns `None`.
The calls after the client's next step return the mask.

It holds the gameplay controls only. The client's own controls, such as the
menu and map keys, are not in it. `local_held` runs on any thread, such as
in a frame of the [interface](interface.md).

```rust
use enshwick::controls::GameAction;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let input = load.input()?;
    load.on_frame(move |frame| {
        if input.local_held().is_some_and(|held| held.contains(GameAction::Block)) {
            frame.label("Blocking");
        }
    })
}
```

```luau
enshwick.ui.on_frame(function(frame)
    local held = enshwick.input.local_held()
    if held ~= nil and table.find(held, "Block") then
        frame:label("Blocking")
    end
end)
```

## Which bit is which

The game keeps the held controls as a 64-bit mask in each player's
`PlayerInput`. The running build's reflection names each bit, and the
loader maps each name to its `GameAction`. A bit the build does not name,
or that `GameAction` lacks, shows in `bits()` only.

## What to trust

The held controls are what the player's client sent, and a client can send
any of them, as often as it likes. Read them for what the player could do
with the keys anyway, such as a stance or a hint. Never let them decide
rights, rewards or an admin step. Those come from the server's own state,
or from a [player action](actions.md), which the server checks on every
run.

## Cached local sampler diagnostics

`Input::local_status()` (Luau: `enshwick.input.local_status()`) reports the loader's cached sampler metadata on any thread.
It shows whether a mod requested sampling, whether its layout and system link are
available, and whether its read fuse is enabled. It also reports whether a sample
exists and remains fresh. `age_ms` is `None` before any sample. In Luau, this is `nil`. A stale or disabled
sample can still have an age. Luau represents the age as a decimal string, as
it represents other `u64` values. A zero held mask is a valid sample.

This getter does not request sampling. Only the existing `local_held()` call
starts it. The flags are weakly consistent observations of cached metadata.
They are not world or session identity, or proof of input execution.
They do not explain why a player row or component was not sampled.
No new game memory is read. The getter is client-only and returns
`WorldError::Unavailable` on a server. It requires ABI 12.1 or newer.
An older loader rejects this SDK at mod initialization before accessing its
appended host slot.

```lua
local status = enshwick.input.local_status()
if status.sample_present then
    enshwick.log.info(`Cached sample age: {status.age_ms} ms`)
end
```

## Reading the client's state in one copy

On a client, `input.client_snapshot()` returns a `ClientSnapshot`. It holds
what the interface read in its last frame: the open HUD screen, the screen
it returns to, the open screen's host and the tab the in-game menu shows. It
also holds the local player, its held controls and the world's name. One
call copies all of it at once, on any thread.

The menu's tab and the HUD screen are two different things. The `V` key
opens the crafting screen, and `E` then shows the backpack's tab while the
HUD screen stays `PlayerCrafting`. So read `menu_tab()` for the page the
player sees. It names the tab as the screen it stands for: `Social`, `Map`,
`Journal`, `PlayerCrafting`, `Backpack`, `Character` or `SkillTree`. It is
`None` while no menu shows.

`session()` counts the client's sessions from 1. It grows when the
client's game object, the local player or the world changes, so two
snapshots with the same value belong to one session. `state()` says how old
the reads are: `Never` before the first frame, `Fresh` within a second,
`Stale` after that, as while a world loads, and `Off` when the build lacks
the reads. `age_ms()` gives the age in milliseconds.

The first call starts the reads, so it returns `Never`. A part the running
build lacks returns `None`, and `reads(part)` tells "none now" from
"unread". The call returns `WorldError::Unavailable` on a server.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let input = load.input()?;
    load.on_frame(move |frame| {
        if let Ok(snapshot) = input.client_snapshot()
            && snapshot.menu_tab() == Some("Backpack")
        {
            frame.label("The backpack shows");
        }
    })
}
```

```luau
enshwick.ui.on_frame(function(frame)
    local snapshot = enshwick.input.client_snapshot()
    if snapshot.menu_tab == "Backpack" then
        frame:label("The backpack shows")
    end
end)
```

In Luau, `reads` lists the parts the build reads by name, and `session`
and `age_ms` are decimal strings, as other 64-bit values are.

## Driving the client's own screens

On a client, `input.client_command(&command)` asks the game to open or
close one of its screens, or to show a tab of the in-game menu. It returns
a `CommandId`. The call only queues the command. `input.command_status(id)`
then tells what came of it:

| State | Means |
| --- | --- |
| `Pending` | queued; the game has not taken it yet |
| `Applied` | the game took it, such as its interface queue the screen change |
| `Observed` | the client's own state shows the screen or tab |
| `Cancelled(reason)` | it ended without its effect |
| `Unavailable` | the build lacks what it needs |

A command belongs to one session and, if you name one, to one HUD screen.
Take both from a [snapshot](#reading-the-clients-state-in-one-copy). The
loader cancels a command when the session changes, when another screen
shows, when the player presses a control, when the mod turns off, and after
two seconds without its effect. One command of each kind waits per mod; a
second answers `WorldError::Refused` until the first ends.

The screens a command opens are the ones the game opens from a key in the
world: `SystemMenu`, `InGameMenu`, `Map`, `Journal`, `Character`,
`PlayerCrafting`, `Backpack`, `SkillTree`, `TextChat` and `Social`. The
loader runs every command in the client's own frame, never inside a mod's
callback.

```rust
use enshwick::input::{ClientCommand, CommandAction};

fn open_backpack(input: enshwick::input::Input) {
    let Ok(snapshot) = input.client_snapshot() else {
        return;
    };
    let command = ClientCommand::new(CommandAction::OpenScreen("Backpack".to_owned()), snapshot.session())
        .on_screen("InGame");
    if let Ok(id) = input.client_command(&command) {
        enshwick::info!("queued {id:?}");
    }
}
```

```luau
local snapshot = enshwick.input.client_snapshot()
local id = enshwick.input.client_command({
    kind = "open_screen",
    session = snapshot.session,
    expected_screen = "InGame",
    target = "Backpack",
})
```

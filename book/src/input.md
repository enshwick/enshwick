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
The client reads the mask only once a mod opens its input with
`load.input()`. A Luau mod's `enshwick.input` opens as the mod starts, and
only when one of its scripts names `input`. A Luau mod that never names it
starts no reads, and neither does a data pack without scripts.
The client's first step after that reads it, so a mod's first frame in a
game has the mask. The read costs one row of the interface's player query
per client step, and only while a mod has its input open.

`local_held()` returns `None` in these cases:

- on a server;
- outside a game;
- before the client's first step after the input opened;
- while no step ran for a second, as in the pause menu;
- on a game build without the read.

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

## Reading and changing the game's own key bindings

On a client, `input.binding(&action)` returns the keys the player's keyboard
and mouse bindings give one of the game's controls. The action is a
`GameplayAction` of the [typed API](concepts/typed-stability.md), so it
needs the `typed` feature in Rust. The returned `Binding` has a first and a
second key. Each is a `BoundKey`, or `None` for no key:

| `BoundKey` | What it holds |
| --- | --- |
| `Key(Key)` | a key or mouse button with its modifiers, in the form a [control](controls.md) takes |
| `Reserved(name)` | a key no mod control may take, by its US name: Escape, a Windows key or a modifier alone, such as `LeftShift` |
| `Other` | input a `Key` does not name, such as the mouse wheel |

These are the keys the game's input and its key glyphs use, the player's
own changes included. The interface reads them at its first frame and every
16 frames after, so a change the player applies shows within a quarter
second.

`input.rebind(&action, key)` binds `key` as the action's first key and
leaves the second. The game's input and key glyphs follow at the next
frame. Each time the player applies a change on the Controls page, the game
rebuilds its bindings from the player's scheme. The loader then writes the
rebind again at its next look, so one call holds. The Controls page always
shows the player's own keys, and the game saves only those.

A rebind belongs to the mod that made it. It ends when that mod calls
`input.reset_binding(&action)` or turns off. The action then takes another
mod's rebind of it, or else the player's own first key. When two mods
rebind one action, the one that rebound it last wins.

Both calls answer `Unavailable` until the interface's first frame, so a mod
makes them from a frame handler:

```rust
use enshwick::controls::Key;
use enshwick::input::{BindingError, BoundKey};
use enshwick::typed::input_mapping::runtime::GameplayAction;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let input = load.input()?;
    let jump = Key::parse("J")?;
    let mut done = false;
    load.on_frame(move |_| {
        if done {
            return;
        }
        if let Ok(binding) = input.binding(&GameplayAction::ContextualAction) {
            if let Some(BoundKey::Key(key)) = binding.first {
                enshwick::info_once!("interact is {key}");
            }
            let rebound = input.rebind(&GameplayAction::Jump, jump);
            done = rebound != Err(BindingError::Unavailable);
        }
    })
}
```

```luau
local done = false
enshwick.ui.on_frame(function()
    if done then
        return
    end
    local read, jump = pcall(function()
        return enshwick.input.binding("Jump")
    end)
    if read then
        enshwick.log.info(`jump is {jump.first}`)
        enshwick.input.rebind("Jump", "J")
        done = true
    end
end)
```

In Luau an action is the variant's name, such as `"ContextualAction"`. A key
is its text, such as `"Shift+E"`. A reserved key is its name, and other
input is `"Other"`.

Each call fails with a `BindingError`:

- `Unavailable` on a server, before the interface drew its first frame, and
  on a game build where the loader does not find the bindings;
- `NoAction` for an action the running build does not name;
- `Refused` from `rebind` for a key whose modifier the game's bindings do
  not list.

## Cached local sampler diagnostics

`Input::local_status()` (Luau: `enshwick.input.local_status()`) reports the loader's cached sampler metadata on any thread.
It shows whether a mod requested sampling, whether its layout and system link are
available, and whether its read fuse is enabled. It also reports whether a sample
exists and remains fresh. `age_ms` is `None` before any sample. In Luau, this is `nil`. A stale or disabled
sample can still have an age. Luau represents the age as a decimal string, as
it represents other `u64` values. A zero held mask is a valid sample.

This getter does not request sampling itself. Opening the input or a
`local_held()` call starts it. The flags are weakly consistent observations of cached metadata.
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
what the interface read in its last frame. That is the open HUD screen, the
screen it returns to, the open screen's host and the in-game menu's tab. It
also holds the local player, its held controls and the world's name. One
call copies all of it at once, on any thread.

The menu's tab and the HUD screen are two different things. The `V` key
opens the crafting screen. The `E` key then shows the backpack's tab, while
the HUD screen stays `PlayerCrafting`. So read `menu_tab()` for the page the
player sees. It names the tab as the screen it stands for, and it is `None`
while no menu shows:

| Tab | Shows |
| --- | --- |
| `Social` | the players of the world |
| `Map` | the map |
| `Journal` | the journal |
| `PlayerCrafting` | the crafting menu |
| `Backpack` | the backpack |
| `Character` | the character |
| `SkillTree` | the skill tree |

`session()` counts the client's sessions from 1. It grows when the
client's game object, the local player or the world changes. So two
snapshots with the same value belong to one session. `state()` says how old
the reads are, and `age_ms()` gives the age in milliseconds:

| State | Means |
| --- | --- |
| `Never` | no frame drew a game yet |
| `Fresh` | read within a second |
| `Stale` | older, as while a world loads |
| `Off` | the build lacks the reads |

`highlighted_recipe()` names the recipe the crafting menu's details column
shows. It follows a click on a tile and the arrow keys; the mouse pointer
over a tile does not move it. It is `None` while no crafting menu draws,
and while the open menu highlights no recipe.

The client reads the snapshot once a mod opens its input, as it does the
held controls. So a mod's first frame in a game has it. The state is
`Never` in two cases:

- before the client's first interface step after the mod opened its input;
- while no frame drew a game since, as in the main menu before the first
  world.

The reads cost one look at the interface's screen state and menu tab per
frame, which client commands share, and a copy of four words. The held
controls cost their own row, as for `local_held()`. A part the running
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

In Luau, `reads` lists the parts the build reads by name. The `session` and
`age_ms` fields are decimal strings, as other 64-bit values are.

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
| `Unavailable` | the build lacks what it needs, or the open menu lacks the recipe |

A command belongs to one session and, if you name one, to one HUD screen.
Take both from a [snapshot](#reading-the-clients-state-in-one-copy). Session
0, from a snapshot taken before the first frame, means the session of the
first frame that runs the command. A command whose effect already shows,
such as opening the screen that is open, is `Observed` at once. The loader
cancels a command in these cases:

- the session changes;
- another screen shows;
- the player presses a control;
- the mod turns off;
- two seconds pass without its effect, in a world or outside one;
- a window command finds its item drawn off (`ItemOff`).

A reason a newer loader gives reads as `CancelReason::Other`, `"other"` in
Luau. In Rust, the `name()` of a reason, a join failure or a join phase is
the name Luau uses, such as `"other_screen"`. So a Rust mod logs an
outcome under the same name. `CommandId::as_u64()` gives the command's
number, the one Luau returns.

One command of each kind waits per mod, and one press of each game action.
A second answers `WorldError::Refused` until the first ends.

A command opens the screens the game opens from a key in the world. These
are the seven tabs of the [snapshot](#reading-the-clients-state-in-one-copy)
and three more:

| Screen | Shows |
| --- | --- |
| `SystemMenu` | the pause menu |
| `InGameMenu` | the in-game menu |
| `TextChat` | the chat line |

The loader runs every command in the client's own frame, never inside a
mod's callback.

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

## Turning the camera and taking a short step

`CommandAction::Look { yaw, pitch }` turns the camera once, and
`CommandAction::Move { direction, hold_ms }` holds a direction for a short
time. The loader writes both into the local player's input right after the
game builds it, so they act through the game's own camera and movement.

| Command | Bounds | Applied | Observed |
| --- | --- | --- | --- |
| `Look` | each turn at most a quarter turn, in radians | with Observed | the frame's look was written |
| `Move` | x right and y forward, length above 0 and at most 1; 1 to 250 ms | the first step holds the direction | the hold ran out |

A command outside its bounds returns `WorldError::Refused`. A look is in
radians before the player's camera sensitivity. The camera turns by the
look times the sensitivity, so a turn of an exact angle divides by it. A
positive yaw turns right.

Both run only on the world's own screen, `InGame`. The player's own input
wins: a stick or mouse move, or a pressed control, cancels the command with
`PlayerInput`. The command is also cancelled in these cases:

- another screen shows;
- a new session starts;
- the game blocks the player's input, with `InputBlocked`;
- the mod turns off.

When a move ends for any reason, the stick is the game's own again in the
same step. One look or move runs at a time across all mods. Another mod's
command waits until the first ends.

```rust
use enshwick::input::{ClientCommand, CommandAction};

fn step_forward(input: enshwick::input::Input) {
    let Ok(snapshot) = input.client_snapshot() else {
        return;
    };
    let action = CommandAction::Move { direction: [0.0, 1.0], hold_ms: 250 };
    let _queued = input.client_command(&ClientCommand::new(action, snapshot.session()));
}
```

```luau
local snapshot = enshwick.input.client_snapshot()
enshwick.input.client_command({
    kind = "move",
    session = snapshot.session,
    direction = { 0, 1 },
    hold_ms = 250,
})
```

## Pressing a game action

`CommandAction::Press { action, hold_ms }` holds one `GameAction` in the
local player's input for 1 to 2000 ms, as its key would. The loader sets
the action's bit right after the game builds the input, on every step of
the hold. A hold of 1 ms holds it for one step, a tap.

| Command | Bounds | Applied | Observed |
| --- | --- | --- | --- |
| `Press` | a `GameAction` the running build names; 1 to 2000 ms | the first step holds the bit | the hold ran out |

The game acts on the form of an action its key sets, and the loader sets
only the action a press names. Some actions have tap, hold and release
forms. For those, press the form the key gives. These were seen on client
26430625, on a hosted world and on a client joined to a server:

| To do this | Press |
| --- | --- |
| interact with the prompt's target | `ContextualActionTap` |
| attack, mine or use the held item | `MainhandActionTap` and `MainhandActionRelease` |
| jump | `Jump` |
| sprint | `Sprint` |
| use the first quick-use slot | `QuickUse0` |

A press of `MainhandAction` alone does nothing: that bit is the key held,
and the game attacks on its tap.

A press runs only on the world's own screen, `InGame`. A press of the
interact key may open the screen of what the player faces during the hold,
such as a storage chest's. That screen is the press's effect, so the press
is then `Observed` and stops holding. The interact key's actions are
`ContextualAction` and its tap and hold forms. Any other screen cancels a
press, such as the map the player opens or a screen another command opens.
Otherwise it is cancelled in the same cases as a move, with the same
reasons. A control the player
presses cancels it with `PlayerInput`; a control the player already held
when the press started does not. The bits presses set are not the player's
own input, so they cancel no command. When a press ends for any reason,
the bit is the game's own again in the same step.

Several presses hold at once, beside a look or a move, so a mod can sprint
while it moves. A mod holds one press of each action and four presses at
most: another returns `WorldError::Refused` until one ends. Up to eight
presses hold at once across all mods. Another waits until one ends, and
after two seconds of waiting it is cancelled with `TimedOut`.

The server sees a press as the player's key: `Input::held` names it there,
and `Input::local_held` names it on the client. The server checks every
action it gets, as it does for a player's own key.

```rust
use enshwick::controls::GameAction;
use enshwick::input::{ClientCommand, CommandAction};

fn interact(input: enshwick::input::Input) {
    let Ok(snapshot) = input.client_snapshot() else {
        return;
    };
    let action = CommandAction::Press { action: GameAction::ContextualActionTap, hold_ms: 50 };
    let _queued = input.client_command(&ClientCommand::new(action, snapshot.session()));
}
```

A script names the action by its game name, as a control's `link` does:

```luau
local snapshot = enshwick.input.client_snapshot()
enshwick.input.client_command({
    kind = "press",
    session = snapshot.session,
    action = "ContextualAction_Tap",
    hold_ms = 50,
})
```

## Highlighting a recipe

`CommandAction::HighlightRecipe(recipe)` asks the open crafting menu to
highlight a recipe, as a click on its tile does. It crafts nothing, pins
nothing and moves no item. The command waits until a crafting menu draws.
It is `Observed` once the snapshot's `highlighted_recipe()` names the
recipe. It is `Unavailable` when the open menu does not list the recipe,
such as a recipe of another workshop.

```luau
local function highlight(recipe: number)
    local snapshot = enshwick.input.client_snapshot()
    return enshwick.input.client_command({
        kind = "highlight_recipe",
        session = snapshot.session,
        recipe = recipe,
    })
end
```

## Setting the waypoint and pinging the map

`CommandAction::SetWaypoint(position)` sets the local player's waypoint at
a world position, x, y and z in metres, or moves it there.
`CommandAction::ClearWaypoint` clears it, and `CommandAction::Ping(position)`
pings a position for the players of the world and leaves the waypoint as
it is. Each one asks the server as the map's own waypoint and ping do. The
loader writes the request into the local player's input in the next game
step, and the server takes it as the player's own.

| Command | Bounds | Observed |
| --- | --- | --- |
| `SetWaypoint` | a finite position below 2,147,483,648 metres on each axis, inside the map | the step's input carries it to the server |
| `ClearWaypoint` | none | the same |
| `Ping` | a finite position below 2,147,483,648 metres on each axis, inside the map | the same |

A position that is not finite returns `WorldError::Refused`. So does a
position whose metres on an axis reach 2,147,483,648 (2^31) either way, the
range of the game's fixed-point world positions. So does a position outside
the map's frame in a hosted world. The loader reads that frame shortly after
a mod first takes a client snapshot or queues a client command. A client
joined to a server has no frame, so there the loader checks only the
fixed-point range. The
server's waypoint follows the request.
[`load.on_waypoint`](map.md#hearing-waypoints-and-pings) and
[`map.waypoint`](map.md#waypoints) tell when it changed. They run on the
server's side, so on a client only in a hosted game.

The commands run on any screen, such as the map's, and the player's input
does not cancel them. One waypoint or ping runs at a time across all mods.
Another waits until the first is written, and after two seconds of waiting
it is cancelled with `TimedOut`. A mod has one waypoint or ping open at a
time.

These commands write what the game's own code writes, as read on client
26430625 and server 26430253.

```rust
use enshwick::input::{ClientCommand, CommandAction};

fn mark(input: enshwick::input::Input, at: [f64; 3]) {
    let Ok(snapshot) = input.client_snapshot() else {
        return;
    };
    let _queued = input.client_command(&ClientCommand::new(CommandAction::SetWaypoint(at), snapshot.session()));
}
```

```luau
local snapshot = enshwick.input.client_snapshot()
enshwick.input.client_command({
    kind = "ping",
    session = snapshot.session,
    position = vector.create(4096, 80, 5120),
})
```

## Joining a server and leaving it

`CommandAction::Join { server, password }` joins a dedicated server from the
main menu, as the game's own server list does. `server` is the server's
`host:query_port`, an address or a name. `password` is the server's
password, `None` for none. The loader hands the password to the game's own
join, which keeps only a hash of it. The loader never keeps, logs or returns
it, and its `Debug` shows none of it. The game keeps no passwords of its
own, so no call reads a saved one.

A join starts at the main menu with a character selected, with session 0.
The loader refuses a join while another join or leave runs, or while the
client is in a world. It never turns a join into a leave.

The loader first asks the server's query port for the server's Steam id and
game port, which the game's join needs. A server that does not answer fails
the join with `Unreachable` before anything reaches the game. Each join
adds the server to Steam's history, as every join from the game does.

`CommandStatus::phase` follows the join at the game's own pace. No two-second
limit cancels a join:

| Phase | Means |
| --- | --- |
| `Queued` | the loader asks the server for its address |
| `Menu` | the main menu took the join and starts play |
| `Connecting` | play started; the client connects |
| `Loading` | connected; the client loads the world |
| `InWorld` | the client is in the server's world; the join is `Observed` |
| `Left` | the client left play, back to the main menu |

A join that ends back at the main menu is `CommandState::Failed(failure)`.
The failure is what the client can tell:

| Failure | Means |
| --- | --- |
| `Unreachable` | the query port did not answer with the server's address |
| `Refused` | the loader's join check refused it: the server runs mods this client lacks |
| `Full` | the server was full when it answered |
| `WrongPassword` | the server asks for a password and the mod gave none |
| `TimedOut` | the game did not start play within 20 seconds |
| `Unknown` | any other reason; the client does not show it |

The game also names a server not found, reserved or on another version.
The loader does not read these reasons yet, so they show as `Unknown`.

`input.cancel_command(id)` cancels a join. Once the game took the join, the
loader also takes the client back to the main menu, out of a world that
loads too. A join that times out goes back the same way. A mod that turns
off leaves its join to the game. `CommandAction::Leave` takes the client
from a world back to the main menu, as the pause menu's own entry does. It
is `Observed`, in phase `Left`, once the main menu shows. The loader refuses
a leave outside a world, or while another join or leave runs. While the client joins a server through
the loader, and while it is in that server's world, the snapshot's
`server()` names the server.

```rust
use enshwick::input::{ClientCommand, CommandAction, Password};

fn join(input: enshwick::input::Input) -> Option<enshwick::input::CommandId> {
    let action = CommandAction::Join {
        server: "203.0.113.5:15637".to_owned(),
        password: Some(Password::new("open sesame")),
    };
    input.client_command(&ClientCommand::new(action, 0)).ok()
}
```

```luau
local id = enshwick.input.client_command({
    kind = "join",
    session = "0",
    target = "203.0.113.5:15637",
    password = "open sesame",
})
enshwick.ui.on_frame(function()
    local status = enshwick.input.command_status(id)
    enshwick.log.info(`{status.state} {status.phase}`)
end)
```

## Hosting a world from the main menu

`CommandAction::Host { world, character, online }` hosts one of the
player's worlds from the main menu, as the menu's own Play does. `world`
and `character` are places in the main menu's lists of worlds and
characters, counted from 0 and below 16. A place is played from the save
storage that holds a save there, the local folder or Steam's cloud. When both
hold one, the host plays the save in Steam's cloud. `online` opens the world to other
players, and `false` plays it offline. A host starts at the main menu with
session 0, as a join does. The loader refuses a host while a join, a host
or a leave runs, or while the client is in a world.

`CommandStatus::phase` follows play through the phases of a
[join](#joining-a-server-and-leaving-it). Once it reaches `InWorld`, the
host is `Observed`. A host that ends back at the main menu is `Failed(Unknown)`. A
host that does not start play within 20 seconds is `Failed(TimedOut)`.

These posts are the main menu's own commands, read on client 26430625.
When the save system does not take
the world or the character, the menu keeps its last choice, so play may
start with that one.

```rust
use enshwick::input::{ClientCommand, CommandAction};

fn host(input: enshwick::input::Input) -> Option<enshwick::input::CommandId> {
    let action = CommandAction::Host { world: 0, character: 0, online: false };
    input.client_command(&ClientCommand::new(action, 0)).ok()
}
```

```luau
local id = enshwick.input.client_command({
    kind = "host",
    session = "0",
    world = 0,
    character = 0,
})
```

## Opening the Mods screen

`CommandAction::OpenMods` opens the loader's own Mods screen at its list of
mods. It opens over the main menu or the pause menu, whichever shows. A
command made while neither menu shows waits, and after two seconds it is
cancelled with `TimedOut`. While the Mods screen or both menus' entries
are turned off, the call returns `WorldError::Unavailable`. `CommandAction::OpenMod(id)` opens a mod's page,
and `CommandAction::OpenModSettings(id)` presses its settings gear. Both
name the mod by its id and open the screen first. An id that no loaded mod
has returns `WorldError::Refused`.

The snapshot's `loader_screen()` names the page that shows, at the main
menu too, before any world. `loader_screen_mod()` names the page's mod
when this client loads it, so a page of a mod only the server runs names
none. A command is `Observed` once its page shows. A gear may open the joined server's settings of a mod
that has nothing to set on this client, and that page counts too.

| `LoaderPage` | Shows |
| --- | --- |
| `ModList` | the list of mods |
| `ModPage` | a mod's page, with its settings in this world |
| `ModServerSettings` | the joined server's settings of a mod |
| `ServerModList` | the joined server's mods with settings |
| `ModBindings` | the key bindings of a mod the server runs and this client lacks |
| `ModMismatch` | the mods that differ from a server that refused this client |

```luau
enshwick.input.client_command({
    kind = "open_mod_settings",
    session = enshwick.input.client_snapshot().session,
    target = "you.trail",
})
```

## Pressing and typing in a server mod's window

A server's mod shows [windows](actions.md#windows-from-the-server) on players' clients.
On the client, three commands act on the newest of them as the player's
clicks and typing would:

| Command | Does |
| --- | --- |
| `PressWindowItem { window, item }` | clicks a button, flips a list's switch or an on-or-off field |
| `PickWindowItem { window, item, option }` | picks the option of a field's list by the name the window shows |
| `SetWindowText { window, item, text }` | sets a field's text, at most 255 bytes |

`window` is `mod:window`, the mod's id and the window's. `item` comes from
the action ids the window's mod declared:

| Item id | Names |
| --- | --- |
| `<action>` | the button that runs the action, drawn whole or in its form |
| `<action>/<n>` | the field of the action's argument `n`, from 0 |
| `<action>@<account>` | a list's switch for one player, by the account key |
| `<action>#<n>` | the window's `n`th row button that runs the action, from 0 |
| `@cancel`, `@close`, `@map` | a form's Cancel, the window's Close, the map's button |

`input.client_windows()` lists the items of every window the client holds,
with their kind, label and the line they stand under, such as the row's
player. Only the newest window shows and takes commands, and its
`shown` is true. While it steps aside for the Map screen, a command waits
until it shows again. While a button's form is open, the list holds the
form's fields, its Run and Cancel, and Close.

A command is `Observed` once the window shows it: the press clicked, the
option is picked, the field reads the text. An item the window that shows
lacks is `Unavailable`. A command on an item the window draws off, such as
a Run while a field is empty, is cancelled with `ItemOff`. The list's `off`
shows only the player's rights and a map the build cannot open, not the
empty field. A pick of a name
the list lacks is cancelled the same way. A number field takes no command.

```rust
use enshwick::input::{ClientCommand, CommandAction, Input};

fn announce(input: Input, line: &str) -> Option<()> {
    let session = input.client_snapshot().ok()?.session();
    let window = "you.admin:panel".to_owned();
    let text = CommandAction::SetWindowText { window: window.clone(), item: "say/0".to_owned(), text: line.to_owned() };
    input.client_command(&ClientCommand::new(text, session)).ok()?;
    let run = CommandAction::PressWindowItem { window, item: "say".to_owned() };
    input.client_command(&ClientCommand::new(run, session)).ok()?;
    Some(())
}
```

```luau
for _, item in enshwick.input.client_windows() do
    if item.shown and item.kind == "button" and item.line == "Bob" then
        enshwick.input.client_command({
            kind = "press_window_item",
            session = enshwick.input.client_snapshot().session,
            window = item.window,
            item = item.item,
        })
    end
end
```

## Answering the loader's dialog

`CommandAction::AnswerDialog { dialog, response }` answers the loader's
dialog that shows on the client, as a click on its button would. That is a mod's dialog
from [`ui.dialog` or `ui.ask`](interface.md#dialogs), or one of the loader's own,
such as a warning. It runs at the main menu and in a world.

| Snapshot | Holds |
| --- | --- |
| `dialog_buttons()` | the responses of the dialog's buttons, empty while none shows, `None` where dialogs do not show in game |
| `dialog_mod()` | the id of the mod whose dialog shows, `None` for the loader's own |
| `dialog()` | the number of the dialog that shows, 0 while none shows |

The command names the dialog by the snapshot's number, so an answer never
reaches a dialog that showed after the snapshot. It is `Observed` once the
dialog closes with that answer, and the dialog's mod hears it as from the
player. While the named dialog does not show, the command returns
`WorldError::Unavailable`, and it ends `Unavailable` when that dialog
closes before the answer lands. A response the dialog has no
button for returns `WorldError::Refused`. A dialog the loader shows as a
system message box, where dialogs do not show in game, takes no command.

```rust
use enshwick::input::{ClientCommand, CommandAction, Input};
use enshwick::ui::Response;

fn accept(input: Input) -> Option<()> {
    let snapshot = input.client_snapshot().ok()?;
    if !snapshot.dialog_buttons()?.contains(&Response::Ok) {
        return None;
    }
    let ok = CommandAction::AnswerDialog {
        dialog: snapshot.dialog(),
        response: Response::Ok,
    };
    input.client_command(&ClientCommand::new(ok, snapshot.session())).ok()?;
    Some(())
}
```

```luau
local snapshot = enshwick.input.client_snapshot()
if snapshot.dialog_buttons and table.find(snapshot.dialog_buttons, "Ok") then
    enshwick.input.client_command({
        kind = "answer_dialog",
        session = snapshot.session,
        dialog = snapshot.dialog,
        answer = "Ok",
    })
end
```

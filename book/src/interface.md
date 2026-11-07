<!--
type: how-to
description: A mod's client part draws windows, rows and dialogs with the game's own widgets, so its panels, fonts, sounds and button labels look like the game's.
-->

# The interface

A mod's client part draws windows, rows and dialogs with the game's own
widgets, so what it shows looks like the game. It has the same panels,
fonts, settings rows, sounds and button labels in the player's language.

## Clients only

Everything here is client only. On a server, these calls return
[`Off`](concepts/lifecycle.md) with "on clients only":

- `load.ui()`
- `load.on_frame`
- `load.on_hud`
- `load.extend_screen`

## Frames

The game redraws its interface every frame from code, and a mod draws its own
the same way. [`load.on_frame(draw)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ui.rs)
registers a callback during `init`. It runs once per frame, on the thread
that draws the interface, from the title screen on. It draws through the
`Frame` it is handed. A widget's state lives with the mod: a toggle draws the
`bool` it is handed and returns true in the frame the player changes it.

```rust
use enshwick::ui::{Dropdown, Label, Slider, Style, Toggle};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let mut open = true;
    let mut fast = false;
    let mut volume = 0.5f32;
    let mut mode = 0usize;
    load.on_frame(move |frame| {
        if !open {
            return;
        }
        frame.window("options", "Options", |w| {
            w.label(Label::new("Changes apply at once.").style(Style::Secondary).wrap());
            w.toggle(Toggle::new("Fast travel"), &mut fast);
            w.slider(Slider::new("Volume", 0.0..=1.0).steps(11), &mut volume);
            w.dropdown(Dropdown::new("Labels", &["Off", "Owner", "Everyone"]), &mut mode);
            w.progress(volume);
            if w.button("Close") {
                open = false;
            }
        });
    })?;
    Ok(())
}
```

```luau
local open, fast, volume, mode = true, false, 0.5, 0
enshwick.ui.on_frame(function(frame)
    if not open then
        return
    end
    frame:window("options", "Options", function(w)
        w:label({ text = "Changes apply at once.", style = "Secondary", wrap = true })
        fast = w:toggle({ label = "Fast travel" }, fast)
        volume = w:slider({ label = "Volume", min = 0, max = 1, steps = 11 }, volume)
        mode = w:dropdown({ label = "Labels", options = { "Off", "Owner", "Everyone" } }, mode)
        w:progress(volume)
        if w:button({ text = "Close" }) then
            open = false
        end
    end)
end)
```

In Luau, a widget is a table of its builder's fields, such as
`{ text = "Close", enabled = false }`. A slider's range is its `min` and
`max`. Lua has no `&mut`, so a toggle, slider or dropdown takes the value and
returns it. It returns the value changed in the frame the player changes it.
A dropdown's index counts from 0, as in Rust. The callback gets the same
frame object on every frame and may use it only during that call. A
window's `body` is a new function each frame, so its frame is a new object.

A window is the game's popup panel over a dimmed screen. It takes the input
while it shows, and it shows for as long as the mod draws it. `id` names it
among the mod's windows. One window is open at a time per callback, and
widgets draw only inside one. The widgets:

| Widget                   | Looks like                                    | Returns                   |
| ------------------------ | --------------------------------------------- | ------------------------- |
| `label`                  | text in a named style, wrapped or centred     |                           |
| `button`                 | the game's button; `Button::enabled(false)`   | true in the clicked frame |
| `toggle`                 | the settings row with Off and On              | true when changed         |
| `slider`                 | the settings slider over a range, with stops  | true when changed         |
| `dropdown`               | the settings choice, at most 64 options       | true when changed         |
| `progress`               | the game's bar, from 0 to 1                   |                           |
| `sound`                  | plays a click, confirmation, back or refusal  |                           |

Rows take `read_only(true)`: they show the value and do not take changes.
A slider whose range has an end that is not finite draws nothing. A reversed
range slides between the same ends. A value that is not finite shows at the
smaller end. `progress` draws a fraction that is not finite as 0. Styles are
the game's named ones:

- `Body`
- `Secondary`
- `H1` to `H3`
- `Warning`
- `Positive`
- `Negative`

The Interface demo button sits on the Mods screen, before the list of mods.
It shows every widget at once in the loader's own demo window, and its last
page shows samples on the HUD. The button shows only while the empty file
`mods/enshwick/developer-tools` exists on that client, a
[developer tool](#developer-tools). On a server, the console line
`enshwick ui-demo <player>` opens the same window on that player's client.
The map gets a marker where the player stood. It works only when the file
exists on the server and on that client when they start. Never leave
the file on a server that players use.

`frame.screen()` says where the player is: the menus, loading, or in game.

A frame copies the texts it is handed, so nothing needs to outlive the call.
A frame allocates nothing on the mod's side but its closure's own work.
A panic in the callback turns the mod off, and the loader closes the window
it left open.

## The HUD

[`load.on_hud(draw)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ui/hud.rs)
registers a callback that runs once per frame while the game draws its HUD.
It draws lines and bars at an `Anchor`: a corner, or the middle of the top or
bottom edge. Items at one anchor stack in the order drawn, in a column the
width of a few words. What a frame draws shows that frame only, so an item
goes away when the mod stops drawing it. The HUD hides with the game's own:
with the HUD turned off, in full-screen menus, in screenshot mode and in a
dialog with a character. Body text gets the game's outlined HUD style.

```rust
use enshwick::ui::{Anchor, Label, Style};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let mut seconds = 120u32;
    load.on_hud(move |hud| {
        seconds = seconds.saturating_sub(1);
        let text = format!("Raid in {}:{:02}", seconds / 60, seconds % 60);
        hud.text(Anchor::TopRight, Label::new(&text).style(Style::Warning));
        hud.progress(Anchor::TopRight, seconds as f32 / 120.0);
    })?;
    Ok(())
}
```

```luau
local seconds = 120
enshwick.ui.on_hud(function(hud)
    seconds = math.max(seconds - 1, 0)
    hud:text("TopRight", { text = string.format("Raid in %d:%02d", seconds // 60, seconds % 60), style = "Warning" })
    hud:progress("TopRight", seconds / 120)
end)
```

[`examples/waypoints`](https://github.com/enshwick/enshwick/tree/main/examples/waypoints)
draws on the HUD what its server half sent over a
[channel](messages.md), and only on a client. It asks
`load.host().build().side` before it calls `on_hud`, since a dedicated server
refuses it. Its test draws the HUD through the fake host with `draw_hud`.

## Panels on the game's screens

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

[`load.extend_screen(screen, edge, draw)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ui/extension.rs)
adds a panel to one of the game's own screens, such as a storage chest's.
`screen` is a `HudScreenType`, the game's own list of screens. `edge` is one
edge of the screen, and the panel sits there, centred along it. The edges
are:

- `Left`
- `Right`
- `Top`
- `Bottom`

`draw` runs once per frame while the player has that screen open. It fills
the panel with the widgets of the `Frame` it is
handed, as a window's body does. The panel shows and hides with the screen.

```rust
use enshwick::ui::{Edge, HudScreenType, Label, Style};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.extend_screen(HudScreenType::StorageChest, Edge::Right, |panel| {
        panel.label(Label::new("Shared with your guild").style(Style::Secondary));
        if panel.button("Sort by name") {
            // Send the server one of the mod's player actions.
        }
    })
}
```

```luau
enshwick.ui.extend_screen("StorageChest", "Right", function(panel)
    panel:label({ text = "Shared with your guild", style = "Secondary" })
    if panel:button({ text = "Sort by name" }) then
        -- Send the server one of the mod's player actions.
    end
end)
```

The game's screens have no place to add to. So the loader draws the panels at
each edge in one window of its own. That window sits on a layer over the
game's screen and under the game's tooltips and the item being dragged. It
shows only once a panel draws a widget in it, so a panel that draws nothing
shows nothing. The panel never moves or changes the game's screen.
`frame.window` is refused inside a panel, which is already one.

A panel cannot go on the screens of play with no window open, where it would
cover the world. For these screens, `extend_screen` returns `Off`:

- `InGame`
- `NoUi`
- `TakeScreenshot`
- the multitools' building screens

The game's buttons stay the game's: a panel's buttons are the mod's own. A
press that changes the world goes to the server as a
[player action](actions.md), which the server checks again.

A panel draws for every storage chest, the mod's own and the game's alike.
To show it only at the mod's own objects, the client part tracks which object
the player opened. For example, its server half can send a message when the
player opens one.
[`examples/chest-panel`](https://github.com/enshwick/enshwick/tree/main/examples/chest-panel)
adds a panel whose button runs a player action, and tests it with
`draw_screen`. On a game build without a screen of that name,
`extend_screen` returns `Off`. When the build lacks the place where the open
screen is read, the panels stay off, and the log and the Mods screen say why.

### Which screen is open

`frame.open_screen()` returns the `HudScreenType` the player has open in that
frame. It returns `InGame` in the world with no screen open. It returns
`None` outside a game, or on a game build where the loader does not find it.

`load.on_screen_changed(handler)` calls the handler on the interface thread
each time the open screen changes. It hands the handler a `ScreenChanged`.
Its fields `before` and `now` name the screen before the change and the one
now. A chest, the map or a survivor's dialog that opens or closes changes
the open screen. The loader reads the screen once a
frame, from the same byte as the panels.

```rust
use enshwick::ui::HudScreenType;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_screen_changed(|change| {
        if change.now == Some(HudScreenType::StorageChest) {
            enshwick::info!("a chest opened");
        }
    })
}
```

```luau
enshwick.ui.on_screen_changed(function(change)
    if change.now == "StorageChest" then
        enshwick.log.info("a chest opened")
    end
end)
```

The screen names its kind only, not the object it belongs to, such as
which chest.

## A prompt on the mod's object

A mod's object gets the game's own prompt when it is placed from a template
the game offers an interaction for, such as a chest's. The mod shapes that
prompt with parts that work on every client, with or without Enshwick:

| What                              | How                                                                       |
| --------------------------------- | ------------------------------------------------------------------------- |
| The prompt's title                | the object's label, [`world.set_label`](world.md#labels)                  |
| No prompt for a player            | refuse `Kind::Open` in [`load.on_interaction`](vetoes.md#interactions)    |
| The mod's own action on the press | the game event `AcceptOfferedInteractionEvent` ([Game events](game-events.md)), its `host_entity_id` the object |

The game's data shows the object's own action, such as opening the chest,
running on the press as well. The mod's action runs beside it on the server.
This is not yet checked in the running game.

## Map markers

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

[`Ui::set_markers`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ui/map.rs)
shows the mod's markers on the map, at most 256. Each has a position in
metres, a label and one of the game's eight custom marker colours. The next
call replaces them all, and `clear_markers` takes them off. It works from any
thread and copies what it is handed. By default a marker uses one of the
game's custom marker types with a label. The compass does not show them.
When the game build lacks a part of the map's layout, the markers stay off
and `set_markers` has no effect. The log and the Mods screen say why.

```rust
use enshwick::ui::{Marker, MarkerColor, Ui};

fn mark_lockers(ui: Ui, lockers: &[([f64; 3], String)]) {
    let markers: Vec<Marker<'_>> = lockers
        .iter()
        .map(|(at, owner)| Marker::new(*at).label(owner).color(MarkerColor::Green))
        .collect();
    let _shown = ui.set_markers(&markers);
}
```

```luau
local function mark_lockers(lockers: { { at: vector, owner: string } })
    local markers = {}
    for _, locker in lockers do
        table.insert(markers, { position = locker.at, label = locker.owner, color = "Green" })
    end
    enshwick.ui.set_markers(markers)
end
```

A marker can also take one of the game's own marker types, for the game's
icon and rules of that type. `ui.map_marker_types()` lists the client's
marker type registry. Each `MapMarkerType` has its key, its sorting group and
the game's flags. The sorting groups are:

- `Dungeons`
- `Locations`
- `FlameRelated`
- `CustomMarker`
- `None`

The flags include `is_fast_travel_destination` and `show_above_fog_of_war`.
The registry is game data and grows with the game,
so pick a type by its flags or its group, not by a fixed key. The colour
shows only on the custom marker types. `set_markers` refuses a type the
registry does not list.

```rust
use enshwick::map::MarkerTypeSorting;
use enshwick::ui::{Marker, Ui};

fn mark_dungeons(ui: Ui, dungeons: &[[f64; 3]]) {
    let types = ui.map_marker_types().unwrap_or_default();
    let Some(dungeon) = types
        .iter()
        .find(|ty| ty.sorting_category == Some(MarkerTypeSorting::Dungeons))
    else {
        return;
    };
    let markers: Vec<Marker<'_>> = dungeons
        .iter()
        .map(|&at| Marker::new(at).marker_type(dungeon.marker_type))
        .collect();
    let _shown = ui.set_markers(&markers);
}
```

## Dialogs

[`Ui::dialog`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ui.rs)
queues a dialog in the game's own style, the one it shows for its own errors.
It has a title, a message, smaller detail text and buttons the game labels in the
player's language. It works from any thread; the answer callback runs on the
thread that draws the interface. Dialogs show one at a time, in order, and
never over one of the game's own.

```rust
use enshwick::ui::{Dialog, Response};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let ui = load.ui()?;
    let dialog = Dialog::new("Survivors", "Move every survivor to this altar?")
        .detail("Survivors with a bed elsewhere keep it.")
        .buttons(&[Response::Ok, Response::Cancel]);
    let _queued = ui.dialog(&dialog, |answer| {
        if answer == Some(Response::Ok) {
            enshwick::info!("moving survivors");
        }
    });
    Ok(())
}
```

```luau
enshwick.ui.dialog({
    title = "Survivors",
    message = "Move every survivor to this altar?",
    detail = "Survivors with a bed elsewhere keep it.",
    buttons = { "Ok", "Cancel" },
}, function(answer)
    if answer == "Ok" then
        enshwick.log.info("moving survivors")
    end
end)
```

`Ui::ask` shows a dialog and waits for the answer, for a thread of the mod's
own. On the thread that draws the interface it returns
`UiError::WrongThread`, since that thread would never draw the dialog. The
answer is `None` when the dialog closed without one.

The buttons are the game's, shown in the game's own order:

- `Ok`
- `Cancel`
- `Retry`
- `Ignore`
- `Reset`
- `Disable`
- `Erase`
- `Overwrite`

When the loader cannot link the interface on a game build, frame callbacks
never run and dialogs show as system message boxes instead.

## The title screen's line

Under the game's version on the title screen, the loader writes its own line
in the same look: `Enshwick 1.0.0-alpha.9 · mods: 6`. The line adds how many
mods are off, and a warning when this game version leaves some of the loader's features
off.

## The Mods screen

The loader adds a "Mods" entry to the title screen's main menu and to the
pause menu, beside Settings. It opens a screen of the loader's own that lists
every mod, with the [extension points](extending.md) it defines and the ones
it extends. Each mod that derives `Config` has a Settings button there, with
nothing to write in the mod. It opens the mod's settings, each player's own
first, then the world's (see
[How a world's file is kept](settings.md#how-a-worlds-file-is-kept)). Mod settings show
nowhere else; the game's world settings have no mod rows. Back, Escape or the
gamepad's B steps back from a mod's settings to the list, and from the list
out. While a text field has the caret, Back takes the caret from the field
and leaves the page open; the next Back steps back.

The list of mods, a mod's settings and the server's settings scroll in the
game's own scroll area. They scroll with its scrollbar, as the game's own
settings do. On a game build where the loader cannot draw that scroll area,
the screen shows a few at a time. It then has "Next Page" and "Previous
Page" buttons, and the log says why.

The screen shows mods and their settings, never their
[player actions](actions.md): a mod offers those through its own
[controls](controls.md) and panels.

The screen says what is off and why. Under a mod, each of its features that
is off has a line with the reason. A mod that is off says so in its heading.
Before the list, each part of the loader that is off has a line. A part, such
as the HUD items or the map markers, turns off at its first panic. The reason
carries the panic's message. The rest of the interface keeps working.

A player who joins a server reads up to three things in chat, in this order:

- the developer tools warning;
- the server's [message of the day](https://enshwick.com/guides/run-a-server-in-docker/#greet-players-when-they-join);
- one line that names the mods, features and parts of the loader that are off.

The reasons are in the server's log and status summary.

## Developer tools

The empty file `mods/enshwick/developer-tools`, read at start, turns on
the tools for making mods: mods with `developer: true` in their manifest and
the interface demo. The rule is the same for a dedicated server, a client and
a game a client hosts for others. Without the file the loader turns a
developer mod off before its `init`, with the reason in the log and on the
Mods screen. A game players join never runs
them unless its owner made the file. A mod author creates the file once on
their own machine. On a client, it goes in the game's folder, the same folder
that holds `mods/`. On a dedicated server, it goes in the server's folder. A
tester's client that runs the probe or the research mod needs the file too.

While the file exists, every player reads `DEVELOPER TOOLS ARE ON` in
upper-case letters. It shows in chat at join, in red at the top of the Mods
screen, and at the end of the main menu's line.

The [fake loader](testing.md) has developer tools off, as a game players
join does. So it refuses a developer mod's `init` with the loader's reason. A
developer mod's tests build it with `FakeHost::developer_tools(true)`.

## Testing

With the `test` feature, the [fake loader](testing.md) runs frame callbacks
with `draw_frame`. It returns what they drew as `Drawn` values. `input`
takes the player's inputs for the next frame, such as `Input::Click` for the
button labelled "Close". The fake loader tests the rest of the interface with
these calls:

- `draw_hud` runs the HUD callbacks and returns their items with their
  anchors.
- `draw_screen` runs the panels of one game screen and returns what each drew
  with its edge.
- `markers` lists the mod's markers.
- `dialogs` lists the dialogs waiting, and `answer` answers the oldest.

A fake built with `.side(Side::CLIENT)` serves the interface. The default
server side turns it away, as a real server does.

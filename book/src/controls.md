# Controls

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

A mod declares the keys its players press; players change them on the
[Mods screen](interface.md#the-mods-screen).

## A first control

```rust
use enshwick::controls::{Control, Key, Phase};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let horn = Control::new("horn", "Sound the horn").key(Key::parse("H")?);
    load.control(horn, |fired| {
        if fired.phase == Phase::Press {
            // Play the horn on this client.
        }
    })?;
    Ok(())
}
```

```luau
enshwick.controls.control({ name = "horn", label = "Sound the horn", key = "H" }, function(fired)
    if fired.phase == "Press" then
        -- Play the horn on this client.
    end
end)
```

The player presses H in the world, and the handler runs on their client.

## When a key fires

The client reads a control's key once a frame while its player is in the
world, a building tool in hand and screenshot mode included: never while the
chat, a menu, the inventory, another of the game's screens or a window or
dialog of Enshwick or a mod is open, and
only while the game's window has the focus. The game keeps the key too: a
mod key that is also a game key does both.

A key is a place on the keyboard, not a letter: `Key::parse("Q")` is the key
left of W on every layout, and the controls page shows it with the name the
player's layout gives it. Names are those of a US keyboard, with Shift, Ctrl
and Alt joined by `+`, such as `Shift+E`, and the mouse buttons `Mouse1` to
`Mouse5`. A control fires when its key goes down with at least its
modifiers held, so a control on `E` fires while the player sprints with
Shift. Of the controls on one key, only those with the most of the held
modifiers fire: with Shift held, `Shift+E` fires and `E` does not. Controls
with as many held modifiers all fire, so with Shift and Ctrl held both
`Shift+E` and `Ctrl+E` do. A control stays down until the key that brought
it down goes up; pressing or letting go of a modifier meanwhile fires
nothing. Escape, the Windows keys, Alt with F4 or Tab, and a modifier alone
cannot be bound, and while Alt is held F4 and Tab fire no control. A numpad
digit fires while Num Lock is on and Shift is up, as Windows reports it with
Shift as the key the numpad has without Num Lock; the numpad's Enter is not a key of its own, as
Windows reports it as Enter.

| Builder                       | What it does                                                         |
| ----------------------------- | -------------------------------------------------------------------- |
| `Control::new(name, label)`   | a control; `name` is 1 to 32 of `[a-z0-9-]`, `label` the page's text |
| `.key(key)`, `.second(key)`   | the default key and a second one                                     |
| `.fires(&[Phase, ...])`       | when it fires, in place of `Phase::Press` alone; empty keeps it      |
| `.hold_for(duration)`         | how long a key stays down before `Phase::Hold`; 500 ms by default    |
| `.link(GameAction)`           | fires on the server when the player's game action fires              |

In Luau, a control is a table of the builder's fields: `name`, `label`,
`key`, `second`, `fires` as a list of phase names, `hold_for` in seconds,
and `link` as the game action's name. A key is its text, such as
`"Shift+E"`; `enshwick.controls.parse(text)` and `from_bits(bits)` return
its own spelling, and `bits`, `code`, `modifiers` and `is_mouse` read it. The handler gets a table of
`control`, `phase` and, on the server, `player`.

`Phase::Press` fires once when the key goes down; `Phase::Hold` once when it
stayed down for the hold time, never from key repeat; `Phase::Release` once
when it comes up. A key pressed and let go between two frames may be missed.

## Changing the world

A control fires on the client, and the server decides what changes. The
handler sends the server one of the mod's [player actions](actions.md) with
`actions.run(id, args)`: the server checks who may run it, runs its handler
and shows the reply as a notice. A control is how a player runs a mod's
action in game; the Mods screen shows no actions.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::controls::{Control, Key};
use enshwick::i18n::Texts;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let actions = load.actions(Texts::default())?;
    actions.add(Action::new("honk", "Honk"), |_| Outcome::done("Honk!"))?;
    load.control(Control::new("horn", "Horn").key(Key::parse("H")?), move |_| {
        let _sent = actions.run("honk", &[]);
    })?;
    Ok(())
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({ id = "honk", label = "Honk" }, function()
    return enshwick.actions.done("Honk!")
end)
enshwick.controls.control({ name = "horn", label = "Horn", key = "H" }, function()
    actions:run("honk", {})
end)
```

## Following a game action

A control linked to a game action reads the game's own decision instead of
a key: `GameAction::ContextualAction` fires whenever the player interacts,
on whatever key, button or gamepad they bound to it. It fires on the
server, for that player, also for a player without Enshwick; the handler
finds the player in `fired.player`. The handler runs on the game thread
after the server's step, where it may change the world.

```rust
use enshwick::controls::{Control, GameAction};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let enter = Control::new("enter", "Enter the car").link(GameAction::ContextualAction);
    load.control(enter, |fired| {
        if let Some(player) = &fired.player {
            // Seat `player` if a car is at hand.
            let _ = player;
        }
    })?;
    Ok(())
}
```

```luau
enshwick.controls.control({ name = "enter", label = "Enter the car", link = "ContextualAction" }, function(fired)
    if fired.player ~= nil then
        -- Seat `fired.player` if a car is at hand.
    end
end)
```

`GameAction` names every member of the game's player input, such as `Jump`,
`Sprint` or `QuickUse0`. A member a game build lacks leaves the control to
its keys, with a line in the log.

## The player's keys

The Mods screen's Controls page lists every mod's controls under the mod's
name. A click on a key waits for the next key or combination, which becomes
the binding; Escape cancels and Unbind clears it. The left mouse button
works the page, so it is bound in the file only. The page warns when two
controls share a key; the game's own keys are not checked.

The page writes `mod-controls.toml` in the game's folder: only the keys the
player changed, each a scan code with its name beside it. A person may edit
the file; a key written as a name, such as `"Shift+E"`, works too, and 0
unbinds. An entry for a control no running mod declares is left alone, with
one line in the log.

```toml
["you.cars:horn"]
key = 36 # J
second = 0 # none
```

## Where it works

Keys need Enshwick on the player's client and a game build whose table finds
the game's current screen; without it the Controls page says the keys are
off. Linked controls run in the game server: the dedicated server, and a
client while it hosts a game, single player included. There a control with a
key and a game action may run its handler on the interface thread and the
game thread at once.

## Testing

With the `test` feature, the [fake loader](testing.md) reads keys as the
client does: `keys(&down, after)` looks at the keys held now, `after` the
last look, and fires each matching control, on a client only;
`game_actions(&player, &held, after)` looks at the game actions the player
holds as the server does once a step, and fires the linked controls. `FakeHost::bind(name, slot, key)` sets one of the player's
keys, `FakeHost::side(Side::CLIENT)` makes the fake a client, which alone
sends player actions,
`controls()` lists the controls with their keys in force, and
`action_runs()` the actions a control sent the server.

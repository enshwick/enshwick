# The interface

A mod's client part draws windows, rows and dialogs with the game's own
widgets, so what it shows looks like the game: the same panels, fonts,
settings rows, sounds and button labels in the player's language. Everything
here is client only; on a server, `load.ui()` and `load.on_frame` return
[`Off`](concepts/lifecycle.md) with "on clients only".

## Frames

The game redraws its interface every frame from code, and a mod draws its own
the same way. [`load.on_frame(draw)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/ui.rs)
registers a callback during `init`; it runs once per frame, on the thread
that draws the interface, from the title screen on, and draws through the
`Frame` it is handed. A widget's state lives with the mod: a toggle draws the
`bool` it is handed and returns true in the frame the player changes it.

```rust
use enshwick_loader_api::ui::{Dropdown, Label, Slider, Style, Toggle};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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

A window is the game's popup panel over a dimmed screen; it takes the input
while it shows, and it shows for as long as the mod draws it. `id` names it
among the mod's windows; one window is open at a time per callback, and
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
Styles are the game's named ones: `Body`, `Secondary`, `H1` to `H3`,
`Warning`, `Positive` and `Negative`.

An administrator sees every widget at once with `/enshwick ui-demo`, which
opens the loader's own demo window on their client; its last page shows
samples on the HUD and a marker on the map where the command was typed.

`frame.screen()` says where the player is: the menus, loading, or in game.

A frame copies the texts it is handed, so nothing needs to outlive the call,
and a frame allocates nothing on the mod's side but its closure's own work.
A panic in the callback turns the mod off, and the loader closes the window
it left open.

## The HUD

[`load.on_hud(draw)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/ui/hud.rs)
registers a callback that runs once per frame while the game draws its HUD.
It draws lines and bars at an `Anchor`, a corner or the middle of the top or
bottom edge; items at one anchor stack in the order drawn, in a column the
width of a few words. What a frame draws shows that frame only, so an item
goes away when the mod stops drawing it. The HUD hides with the game's own:
with the HUD turned off, in full-screen menus, in screenshot mode and in a
dialog with a character. Body text gets the game's outlined HUD style.

```rust
use enshwick_loader_api::ui::{Anchor, Label, Style};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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

## Map markers

[`Ui::set_markers`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/ui/map.rs)
shows the mod's markers on the map, at most 256, each with a position in
metres, a label and one of the game's eight custom marker colours; the next
call replaces them all, and `clear_markers` takes them off. It works from any
thread and copies what it is handed. The markers use one of the game's
custom marker types with a label; the compass does not show them.

```rust
use enshwick_loader_api::ui::{Marker, MarkerColor, Ui};

fn mark_safes(ui: Ui, safes: &[([f64; 3], String)]) {
    let markers: Vec<Marker<'_>> = safes
        .iter()
        .map(|(at, owner)| Marker::new(*at).label(owner).color(MarkerColor::Green))
        .collect();
    let _shown = ui.set_markers(&markers);
}
```

## Dialogs

[`Ui::dialog`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/ui.rs)
queues a dialog in the game's own style, the one it shows for its own errors:
a title, a message, smaller detail text and buttons the game labels in the
player's language. It works from any thread; the answer callback runs on the
thread that draws the interface. Dialogs show one at a time, in order, and
never over one of the game's own.

```rust
use enshwick_loader_api::ui::{Dialog, Response};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let ui = load.ui()?;
    let dialog = Dialog::new("Survivors", "Move every survivor to this altar?")
        .detail("Survivors with a bed elsewhere keep it.")
        .buttons(&[Response::Ok, Response::Cancel]);
    let _queued = ui.dialog(&dialog, |answer| {
        if answer == Some(Response::Ok) {
            enshwick_loader_api::info!("moving survivors");
        }
    });
    Ok(())
}
```

`Ui::ask` shows a dialog and waits for the answer, for a thread of the mod's
own; on the thread that draws the interface it returns
`UiError::WrongThread`, since that thread would never draw the dialog. The
answer is `None` when the dialog closed without one.

The buttons are the game's: `Ok`, `Cancel`, `Retry`, `Ignore`, `Reset`,
`Disable`, `Erase` and `Overwrite`, shown in the game's own order.

When the loader cannot link the interface on a game build, frame callbacks
never run and dialogs show as system message boxes instead.

## The Mods screen

The loader adds a "Mods" entry to the title screen's main menu and to the
pause menu, beside Settings. It opens a screen of the loader's own that lists
every mod and shows the settings of each mod that derives `Config`, as the
game's settings rows, with nothing to write in the mod; see
[Settings](settings.md#the-schema-and-config). Back, Escape or the gamepad's B
steps back from a mod's settings to the list, and from the list out. The world settings show the
same rows under their last tab.

## Testing

With the `test` feature, the [fake loader](testing.md) runs frame callbacks
with `draw_frame`, which returns what they drew as `Drawn` values, and takes
the player's inputs for the next frame with `input`, such as `Input::Click`
for the button labelled "Close". `draw_hud` runs the HUD callbacks and
returns their items with their anchors, `markers` lists the mod's markers,
`dialogs` lists the dialogs waiting and `answer` answers the oldest. A fake
built with `.side(Side::CLIENT)` serves the interface; the default server
side turns it away, as a real server does.

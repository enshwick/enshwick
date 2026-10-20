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
opens the loader's own demo window on their client.

`frame.screen()` says where the player is: the menus, loading, or in game.

A frame copies the texts it is handed, so nothing needs to outlive the call,
and a frame allocates nothing on the mod's side but its closure's own work.
A panic in the callback turns the mod off, and the loader closes the window
it left open.

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

## Testing

With the `test` feature, the [fake loader](testing.md) runs frame callbacks
with `draw_frame`, which returns what they drew as `Drawn` values, and takes
the player's inputs for the next frame with `input`, such as
`Input::Click` for the button labelled "Close"; `dialogs` lists the
dialogs waiting and `answer` answers the oldest. A fake built with
`.side(Side::CLIENT)` serves the interface; the default server side turns it
away, as a real server does.

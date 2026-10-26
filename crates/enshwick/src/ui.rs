//! The client's interface, drawn with the game's own widgets.
//!
//! The game redraws its interface every frame from code, so a mod draws its
//! own the same way: a callback registered with [`Load::on_frame`] runs once
//! per frame on the thread that draws the interface and calls the widgets of
//! [`Frame`] in order. Windows look like the game's own panels, labels take
//! the game's named text styles, and the rows are the game's settings rows.
//! A widget's state lives with the mod: a toggle draws the `bool` it is
//! handed and says when the player changed it.
//!
//! [`Ui::dialog`] queues a dialog in the game's own style, from any thread;
//! [`Ui::ask`] waits for the answer on a thread of the mod's own.
//!
//! [`Load::on_hud`] draws lines and bars on the HUD each frame, at an
//! [`Anchor`], hidden with the game's HUD; [`Ui::set_markers`] shows the
//! mod's markers on the map until it sets others.
//!
//! Everything here is client only: [`Load::ui`] and [`Load::on_frame`] turn
//! a mod's server part away with [`Off`].
//!
//! # Examples
//!
//! ```
//! use enshwick::ui::{Label, Slider, Style};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let mut open = true;
//!     let mut volume = 0.5f32;
//!     load.on_frame(move |frame| {
//!         if !open {
//!             return;
//!         }
//!         frame.window("hello", "Hello", |w| {
//!             w.label(Label::new("A window in the game's style.").style(Style::Secondary));
//!             w.slider(Slider::new("Volume", 0.0..=1.0).steps(11), &mut volume);
//!             if w.button("Close") {
//!                 open = false;
//!             }
//!         });
//!     })?;
//!     Ok(())
//! }
//! ```

use std::sync::Mutex;

use crate::abi::{Status, UiDialog};
use crate::{Host, Load, Off};

mod dialog;
mod frame;
mod hud;
mod map;
mod widgets;

pub use dialog::{Dialog, Response};
pub use frame::Frame;
pub use widgets::{Button, Dropdown, Label, Screen, Slider, Sound, Style, Toggle};

use dialog::{Answer, answered};
use frame::{Drawer, draw_frame};

pub use hud::{Anchor, Hud};
pub use map::{MAX_MARKERS, Marker, MarkerColor};

/// The most options a [`Dropdown`] shows.
pub const MAX_OPTIONS: usize = 64;

/// The interface services for a mod's client part, from [`Load::ui`]; `Copy`,
/// for any thread.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Dialog, Response};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let ui = load.ui()?;
///     let dialog = Dialog::new("Hello", "Enshwick is running.");
///     let _queued = ui.dialog(&dialog, |answer| {
///         enshwick::info!("the player answered {answer:?}");
///     });
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Ui {
    host: Host,
}

/// Why a dialog did not show.
///
/// # Examples
///
/// ```
/// use enshwick::ui::UiError;
///
/// assert_eq!(UiError::Busy.to_string(), "too many dialogs wait");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum UiError {
    /// This process draws no interface, as on a server. A client whose interface the loader could not
    /// link still shows dialogs, as system message boxes.
    #[error("no interface to show it in")]
    Unavailable,
    /// Too many dialogs wait already.
    #[error("too many dialogs wait")]
    Busy,
    /// [`Ui::ask`] on the thread that draws the interface, which would
    /// never draw the dialog it waits for.
    #[error("waited for on the thread that draws the interface")]
    WrongThread,
    /// The loader refused the dialog, such as one without buttons.
    #[error("the loader refused the dialog")]
    Refused,
}

impl Ui {
    /// Queues `dialog` and runs `on_answer` with the player's answer, or
    /// `None` when it closed without one, as when the game quits. Dialogs
    /// show one at a time, in order. Any thread; `on_answer` runs on the
    /// thread that draws the interface.
    ///
    /// # Errors
    ///
    /// A [`UiError`] saying why it did not show; `on_answer` then never
    /// runs.
    ///
    /// # Examples
    ///
    /// See [`Ui`].
    pub fn dialog<F>(&self, dialog: &Dialog<'_>, on_answer: F) -> Result<(), UiError>
    where
        F: FnOnce(Option<Response>) + Send + 'static,
    {
        self.queue(dialog, 0, on_answer)
    }

    /// Shows `dialog` and waits for the player's answer, `None` when it
    /// closed without one. For a thread of the mod's own: the thread that
    /// draws the interface would never draw it.
    ///
    /// # Errors
    ///
    /// A [`UiError`] saying why it did not show.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::{Dialog, Response, Ui};
    ///
    /// fn confirm(ui: Ui) -> bool {
    ///     let dialog = Dialog::new("Safes", "Open every safe in the world?")
    ///         .buttons(&[Response::Ok, Response::Cancel]);
    ///     ui.ask(&dialog) == Ok(Some(Response::Ok))
    /// }
    /// ```
    pub fn ask(&self, dialog: &Dialog<'_>) -> Result<Option<Response>, UiError> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.queue(dialog, UiDialog::WAITING, move |answer| {
            let _sent = sender.send(answer);
        })?;
        Ok(receiver.recv().unwrap_or(None))
    }

    fn queue<F>(&self, dialog: &Dialog<'_>, flags: u32, on_answer: F) -> Result<(), UiError>
    where
        F: FnOnce(Option<Response>) + Send + 'static,
    {
        let api = self.host.api();
        let raw = dialog.to_raw(flags);
        let user = Box::into_raw(Box::new(Answer { on_answer }));
        let status = (api.ui_dialog)(
            self.host.handle(),
            &raw const raw,
            answered::<F>,
            user.cast(),
        );
        if status.is_ok() {
            return Ok(());
        }
        // SAFETY: refused, so the loader keeps no copy of the pointer.
        drop(unsafe { Box::from_raw(user) });
        Err(match status {
            Status::MISSING => UiError::Unavailable,
            Status::REFUSED => UiError::Busy,
            Status::WRONG_THREAD => UiError::WrongThread,
            _ => UiError::Refused,
        })
    }
}

fn client_only(host: Host, what: &str) -> Result<(), Off> {
    if host.build().side == crate::abi::Side::SERVER {
        return Err(Off::new(format!("{what}: on clients only")));
    }
    Ok(())
}

impl Load<'_> {
    /// The interface services: dialogs from any thread. Client only.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server.
    ///
    /// # Examples
    ///
    /// See [`Ui`].
    pub fn ui(&mut self) -> Result<Ui, Off> {
        let host = self.host();
        client_only(host, "the interface")?;
        Ok(Ui { host })
    }

    /// Runs `draw` once per frame of the interface, on the thread that draws
    /// it, from the title screen on; it draws with the [`Frame`] it is
    /// handed. Client only. A panic in `draw` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server or after `init`. When the loader could not link the interface on this game
    /// build, `draw` never runs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_frame<F>(&mut self, draw: F) -> Result<(), Off>
    where
        F: FnMut(&mut Frame<'_>) + Send + 'static,
    {
        let host = self.host();
        client_only(host, "frame callbacks")?;
        crate::load::hand_over(
            Drawer {
                draw: Mutex::new(draw),
            },
            |user| (host.api().ui_frame)(host.handle(), draw_frame::<F>, user),
            "frame callback",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_round_trip_through_their_raw_form() {
        for response in [
            Response::Retry,
            Response::Disable,
            Response::Erase,
            Response::Reset,
            Response::Overwrite,
            Response::Cancel,
            Response::Ok,
            Response::Ignore,
        ] {
            assert_eq!(Response::from_raw(response.bit()), Some(response));
            assert_ne!(UiDialog::BUTTONS & 1 << response.bit(), 0);
        }
        assert_eq!(Response::from_raw(4), None);
        assert_eq!(Response::from_raw(UiDialog::CLOSED), None);
    }

    #[test]
    fn a_dialog_keeps_ok_without_buttons_and_takes_the_ones_given() {
        let plain = Dialog::new("t", "m");
        assert_eq!(plain.to_raw(0).buttons, 1 << UiDialog::OK);
        assert_eq!(plain.buttons(&[]).to_raw(0).buttons, 1 << UiDialog::OK);
        let asked = plain.buttons(&[Response::Cancel, Response::Ok]);
        assert_eq!(asked.to_raw(UiDialog::WAITING).buttons, 0xc0);
        assert_eq!(asked.to_raw(UiDialog::WAITING).flags, UiDialog::WAITING);
    }
}

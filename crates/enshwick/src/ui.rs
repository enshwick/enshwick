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

use core::ffi::c_void;
use core::marker::PhantomData;
use core::ops::RangeInclusive;
use std::sync::Mutex;

use crate::abi::{Header, Status, Str, UiDialog, UiFrame, UiWidget};
use crate::{Host, Load, Off};

mod hud;
mod map;

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

/// An answer the player gives a dialog; each has a button labelled by the
/// game in the player's language.
///
/// # Examples
///
/// See [`Dialog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Response {
    /// "Retry".
    Retry,
    /// "Disable".
    Disable,
    /// "Erase".
    Erase,
    /// "Reset".
    Reset,
    /// "Overwrite".
    Overwrite,
    /// "Cancel".
    Cancel,
    /// "OK".
    Ok,
    /// "Ignore".
    Ignore,
}

impl Response {
    /// The loader's raw answer for this response, such as [`UiDialog::OK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::UiDialog;
    /// use enshwick::ui::Response;
    ///
    /// assert_eq!(Response::Cancel.to_raw(), UiDialog::CANCEL);
    /// ```
    #[must_use]
    pub fn to_raw(self) -> u32 {
        self.bit()
    }

    fn bit(self) -> u32 {
        match self {
            Self::Retry => UiDialog::RETRY,
            Self::Disable => UiDialog::DISABLE,
            Self::Erase => UiDialog::ERASE,
            Self::Reset => UiDialog::RESET,
            Self::Overwrite => UiDialog::OVERWRITE,
            Self::Cancel => UiDialog::CANCEL,
            Self::Ok => UiDialog::OK,
            Self::Ignore => UiDialog::IGNORE,
        }
    }

    /// The response for the loader's answer `raw`; `None` for
    /// [`UiDialog::CLOSED`] or an answer this crate does not know.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::UiDialog;
    /// use enshwick::ui::Response;
    ///
    /// assert_eq!(Response::from_raw(UiDialog::OK), Some(Response::Ok));
    /// assert_eq!(Response::from_raw(UiDialog::CLOSED), None);
    /// ```
    #[must_use]
    pub fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            UiDialog::RETRY => Self::Retry,
            UiDialog::DISABLE => Self::Disable,
            UiDialog::ERASE => Self::Erase,
            UiDialog::RESET => Self::Reset,
            UiDialog::OVERWRITE => Self::Overwrite,
            UiDialog::CANCEL => Self::Cancel,
            UiDialog::OK => Self::Ok,
            UiDialog::IGNORE => Self::Ignore,
            _ => return None,
        })
    }
}

/// A dialog in the game's own style: a title, a message, smaller detail text
/// and buttons the game labels in the player's language.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Dialog, Response};
///
/// let dialog = Dialog::new("Lockers", "Open every locker in the world?")
///     .detail("12 lockers")
///     .buttons(&[Response::Ok, Response::Cancel]);
/// assert_eq!(dialog.title(), "Lockers");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialog<'a> {
    title: &'a str,
    message: &'a str,
    detail: &'a str,
    buttons: u32,
}

impl<'a> Dialog<'a> {
    /// A dialog with an OK button.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub const fn new(title: &'a str, message: &'a str) -> Self {
        Self {
            title,
            message,
            detail: "",
            buttons: 1 << UiDialog::OK,
        }
    }

    /// Smaller text under the message, wrapped.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = detail;
        self
    }

    /// The buttons, which show in the game's own order whatever the order
    /// here; none keeps OK.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub fn buttons(mut self, buttons: &[Response]) -> Self {
        let bits = buttons.iter().fold(0, |bits, r| bits | 1 << r.bit());
        if bits != 0 {
            self.buttons = bits;
        }
        self
    }

    /// The title.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub const fn title(&self) -> &'a str {
        self.title
    }

    fn to_raw(self, flags: u32) -> UiDialog {
        UiDialog {
            header: Header::of::<UiDialog>(),
            title: Str::new(self.title),
            message: Str::new(self.message),
            detail: Str::new(self.detail),
            buttons: self.buttons,
            flags,
        }
    }
}

/// A dialog's answer callback as the loader keeps it until the answer.
struct Answer<F> {
    on_answer: F,
}

unsafe extern "system" fn answered<F>(answer: u32, user: *mut c_void)
where
    F: FnOnce(Option<Response>),
{
    // SAFETY: `user` is the `Answer<F>` boxed for this dialog, handed back
    // exactly once.
    let answer_box = unsafe { Box::from_raw(user.cast::<Answer<F>>()) };
    crate::host::guard(
        "panic in a dialog's answer",
        || (),
        move || (answer_box.on_answer)(Response::from_raw(answer)),
    );
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
    ///     let dialog = Dialog::new("Lockers", "Open every locker in the world?")
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

/// Where the player is while a frame draws.
///
/// # Examples
///
/// ```
/// use enshwick::ui::Screen;
///
/// fn hud_wanted(screen: Screen) -> bool {
///     screen == Screen::InGame
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Screen {
    /// The title screen and the menus before a game.
    Menu,
    /// A game loading, or the intro and outro.
    Loading,
    /// In a game.
    InGame,
}

/// One of the game's named text styles.
///
/// # Examples
///
/// See [`Label`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Style {
    /// Body text.
    #[default]
    Body,
    /// Dimmer, secondary text.
    Secondary,
    /// The largest heading.
    H1,
    /// The second heading.
    H2,
    /// The third heading.
    H3,
    /// A warning.
    Warning,
    /// Good news.
    Positive,
    /// Bad news.
    Negative,
}

impl Style {
    fn to_raw(self) -> u32 {
        match self {
            Self::Body => UiWidget::STYLE_BODY,
            Self::Secondary => UiWidget::STYLE_SECONDARY,
            Self::H1 => UiWidget::STYLE_H1,
            Self::H2 => UiWidget::STYLE_H2,
            Self::H3 => UiWidget::STYLE_H3,
            Self::Warning => UiWidget::STYLE_WARNING,
            Self::Positive => UiWidget::STYLE_POSITIVE,
            Self::Negative => UiWidget::STYLE_NEGATIVE,
        }
    }
}

/// One of the game's interface sounds.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Frame, Sound};
///
/// fn refuse(frame: &mut Frame<'_>) {
///     frame.sound(Sound::Error);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Sound {
    /// A click.
    Click,
    /// A confirmation.
    Confirm,
    /// A step back.
    Back,
    /// A refusal.
    Error,
}

impl Sound {
    fn to_raw(self) -> u32 {
        match self {
            Self::Click => UiWidget::SOUND_CLICK,
            Self::Confirm => UiWidget::SOUND_CONFIRM,
            Self::Back => UiWidget::SOUND_BACK,
            Self::Error => UiWidget::SOUND_ERROR,
        }
    }
}

/// A line of text: its style, wrapped or not, centred or not.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Frame, Label, Style};
///
/// fn heading(frame: &mut Frame<'_>) {
///     frame.label(Label::new("Survivors").style(Style::H2).center());
///     frame.label("Plain body text.");
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label<'a> {
    text: &'a str,
    style: Style,
    flags: u32,
}

impl<'a> Label<'a> {
    /// Body text, on one line.
    ///
    /// # Examples
    ///
    /// See [`Label`].
    #[must_use]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            style: Style::Body,
            flags: 0,
        }
    }

    /// In `style`.
    ///
    /// # Examples
    ///
    /// See [`Label`].
    #[must_use]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Wrapped at the width of its window.
    ///
    /// # Examples
    ///
    /// See [`Label`].
    #[must_use]
    pub const fn wrap(mut self) -> Self {
        self.flags |= UiWidget::WRAP;
        self
    }

    /// Centred.
    ///
    /// # Examples
    ///
    /// See [`Label`].
    #[must_use]
    pub const fn center(mut self) -> Self {
        self.flags |= UiWidget::CENTER;
        self
    }
}

impl<'a> From<&'a str> for Label<'a> {
    fn from(text: &'a str) -> Self {
        Self::new(text)
    }
}

/// A button.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Button, Frame};
///
/// fn buttons(frame: &mut Frame<'_>, saved: bool) -> bool {
///     frame.button(Button::new("Save").enabled(!saved)) || frame.button("Close")
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Button<'a> {
    text: &'a str,
    enabled: bool,
}

impl<'a> Button<'a> {
    /// An enabled button.
    ///
    /// # Examples
    ///
    /// See [`Button`].
    #[must_use]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            enabled: true,
        }
    }

    /// Takes clicks only when `enabled`; a disabled button shows dimmed.
    ///
    /// # Examples
    ///
    /// See [`Button`].
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<'a> From<&'a str> for Button<'a> {
    fn from(text: &'a str) -> Self {
        Self::new(text)
    }
}

/// A settings row with an on and off switch.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Frame, Toggle};
///
/// fn rows(frame: &mut Frame<'_>, fast: &mut bool, synced: bool) {
///     if frame.toggle(Toggle::new("Fast travel").read_only(synced), fast) {
///         enshwick::info!("fast travel {fast}");
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toggle<'a> {
    label: &'a str,
    read_only: bool,
}

impl<'a> Toggle<'a> {
    /// A row that takes changes.
    ///
    /// # Examples
    ///
    /// See [`Toggle`].
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            read_only: false,
        }
    }

    /// Shows the value without taking changes when `read_only`.
    ///
    /// # Examples
    ///
    /// See [`Toggle`].
    #[must_use]
    pub const fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

impl<'a> From<&'a str> for Toggle<'a> {
    fn from(label: &'a str) -> Self {
        Self::new(label)
    }
}

/// A settings row with a slider over a range.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Frame, Slider};
///
/// fn rows(frame: &mut Frame<'_>, altars: &mut f32) {
///     frame.slider(Slider::new("Altars", 1.0..=20.0).steps(20), altars);
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Slider<'a> {
    label: &'a str,
    range: RangeInclusive<f32>,
    steps: u32,
    read_only: bool,
}

impl<'a> Slider<'a> {
    /// A continuous slider over `range`.
    ///
    /// # Examples
    ///
    /// See [`Slider`].
    #[must_use]
    pub const fn new(label: &'a str, range: RangeInclusive<f32>) -> Self {
        Self {
            label,
            range,
            steps: 0,
            read_only: false,
        }
    }

    /// Stops at `steps` evenly spaced values, both ends included; 0 keeps
    /// it continuous.
    ///
    /// # Examples
    ///
    /// See [`Slider`].
    #[must_use]
    pub const fn steps(mut self, steps: u32) -> Self {
        self.steps = steps;
        self
    }

    /// Shows the value without taking changes when `read_only`.
    ///
    /// # Examples
    ///
    /// See [`Slider`].
    #[must_use]
    pub const fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

/// A settings row with a choice among options, at most [`MAX_OPTIONS`].
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Dropdown, Frame};
///
/// fn rows(frame: &mut Frame<'_>, mode: &mut usize) {
///     frame.dropdown(Dropdown::new("Labels", &["Off", "Owner", "Everyone"]), mode);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dropdown<'a> {
    label: &'a str,
    options: &'a [&'a str],
    read_only: bool,
}

impl<'a> Dropdown<'a> {
    /// A row that takes changes.
    ///
    /// # Examples
    ///
    /// See [`Dropdown`].
    #[must_use]
    pub const fn new(label: &'a str, options: &'a [&'a str]) -> Self {
        Self {
            label,
            options,
            read_only: false,
        }
    }

    /// Shows the choice without taking changes when `read_only`.
    ///
    /// # Examples
    ///
    /// See [`Dropdown`].
    #[must_use]
    pub const fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

const fn read_only_flag(read_only: bool) -> u32 {
    if read_only { UiWidget::READ_ONLY } else { 0 }
}

/// One frame of the interface, handed to a callback of
/// [`Load::on_frame`]: draw widgets in order through it. Valid for the
/// callback only, on its thread.
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
pub struct Frame<'f> {
    host: Host,
    token: u64,
    screen: Screen,
    _life: PhantomData<&'f mut ()>,
    _thread: PhantomData<*const ()>,
}

impl Frame<'_> {
    /// Where the player is.
    ///
    /// # Examples
    ///
    /// See [`Screen`].
    #[must_use]
    pub fn screen(&self) -> Screen {
        self.screen
    }

    fn draw(&mut self, widget: &mut UiWidget) -> bool {
        let status = (self.host.api().ui_widget)(self.host.handle(), self.token, widget);
        status.is_ok() && widget.result != 0
    }

    /// Draws a window in the game's style, a panel over the screen that takes
    /// the input while it shows, titled `title`, and its contents with
    /// `body`. `id` names the window among the mod's own; a window shows
    /// for as long as the mod draws it every frame. False when it could not
    /// be drawn, and `body` did not run.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn window(&mut self, id: &str, title: &str, body: impl FnOnce(&mut Self)) -> bool {
        let mut open = UiWidget {
            kind: UiWidget::WINDOW,
            text: Str::new(id),
            detail: Str::new(title),
            ..UiWidget::EMPTY
        };
        let status = (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut open);
        if !status.is_ok() {
            return false;
        }
        body(self);
        let mut end = UiWidget {
            kind: UiWidget::END,
            ..UiWidget::EMPTY
        };
        let _closed = (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut end);
        true
    }

    /// Draws a line of text.
    ///
    /// # Examples
    ///
    /// See [`Label`].
    pub fn label<'t>(&mut self, label: impl Into<Label<'t>>) {
        let label = label.into();
        let _drawn = self.draw(&mut UiWidget {
            kind: UiWidget::LABEL,
            flags: label.flags,
            style: label.style.to_raw(),
            text: Str::new(label.text),
            ..UiWidget::EMPTY
        });
    }

    /// Draws a button; true in the frame it is clicked.
    ///
    /// # Examples
    ///
    /// See [`Button`].
    pub fn button<'t>(&mut self, button: impl Into<Button<'t>>) -> bool {
        let button = button.into();
        self.draw(&mut UiWidget {
            kind: UiWidget::BUTTON,
            flags: read_only_flag(!button.enabled),
            text: Str::new(button.text),
            ..UiWidget::EMPTY
        })
    }

    /// Draws a toggle row showing `on`; true in the frame the player changes
    /// it, with `on` changed.
    ///
    /// # Examples
    ///
    /// See [`Toggle`].
    pub fn toggle<'t>(&mut self, toggle: impl Into<Toggle<'t>>, on: &mut bool) -> bool {
        let toggle = toggle.into();
        let mut widget = UiWidget {
            kind: UiWidget::TOGGLE,
            flags: read_only_flag(toggle.read_only),
            text: Str::new(toggle.label),
            index: u32::from(*on),
            ..UiWidget::EMPTY
        };
        let changed = self.draw(&mut widget);
        if changed {
            *on = widget.index != 0;
        }
        changed
    }

    /// Draws a slider row showing `value`; true in the frame the player
    /// changes it, with `value` changed and kept in the slider's range.
    ///
    /// A range with an end that is not finite draws nothing and gives false.
    /// A reversed range slides between the same ends, and a `value` that is
    /// not finite shows at the smaller end.
    ///
    /// # Examples
    ///
    /// See [`Slider`].
    pub fn slider(&mut self, slider: Slider<'_>, value: &mut f32) -> bool {
        let (start, end) = slider.range.into_inner();
        if !(start.is_finite() && end.is_finite()) {
            return false;
        }
        let (min, max) = (start.min(end), start.max(end));
        let mut widget = UiWidget {
            kind: UiWidget::SLIDER,
            flags: read_only_flag(slider.read_only),
            steps: slider.steps,
            text: Str::new(slider.label),
            value: if value.is_finite() {
                value.clamp(min, max)
            } else {
                min
            },
            min,
            max,
            ..UiWidget::EMPTY
        };
        let changed = self.draw(&mut widget) && widget.value.is_finite();
        if changed {
            *value = widget.value.clamp(min, max);
        }
        changed
    }

    /// Draws a dropdown row showing option `index`; true in the frame the
    /// player picks another, with `index` changed. Options past
    /// [`MAX_OPTIONS`] do not show.
    ///
    /// # Examples
    ///
    /// See [`Dropdown`].
    pub fn dropdown(&mut self, dropdown: Dropdown<'_>, index: &mut usize) -> bool {
        let mut options = [Str::EMPTY; MAX_OPTIONS];
        let count = dropdown.options.len().min(MAX_OPTIONS);
        for (slot, option) in options.iter_mut().zip(dropdown.options) {
            *slot = Str::new(option);
        }
        let mut widget = UiWidget {
            kind: UiWidget::DROPDOWN,
            flags: read_only_flag(dropdown.read_only),
            text: Str::new(dropdown.label),
            options: options.as_ptr(),
            option_count: u32::try_from(count).unwrap_or(0),
            index: u32::try_from(*index).unwrap_or(u32::MAX),
            ..UiWidget::EMPTY
        };
        let changed = self.draw(&mut widget);
        let picked = widget.index as usize;
        if changed && picked < count {
            *index = picked;
            return true;
        }
        false
    }

    /// Draws a bar filled to `fraction`, from 0 (empty) to 1 (full).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::Frame;
    ///
    /// fn loading(frame: &mut Frame<'_>, done: u32, total: u32) {
    ///     frame.progress(done as f32 / total.max(1) as f32);
    /// }
    /// ```
    pub fn progress(&mut self, fraction: f32) {
        let _drawn = self.draw(&mut UiWidget {
            kind: UiWidget::PROGRESS,
            value: if fraction.is_finite() {
                fraction.clamp(0.0, 1.0)
            } else {
                0.0
            },
            min: 0.0,
            max: 1.0,
            ..UiWidget::EMPTY
        });
    }

    /// Plays one of the game's interface sounds.
    ///
    /// # Examples
    ///
    /// See [`Sound`].
    pub fn sound(&mut self, sound: Sound) {
        let _played = self.draw(&mut UiWidget {
            kind: UiWidget::SOUND,
            style: sound.to_raw(),
            ..UiWidget::EMPTY
        });
    }
}

/// A frame callback as the loader keeps it.
struct Drawer<F> {
    draw: Mutex<F>,
}

unsafe extern "system" fn draw_frame<F>(frame: *const UiFrame, user: *mut c_void)
where
    F: FnMut(&mut Frame<'_>),
{
    // SAFETY: the loader passes a valid frame for the call.
    let Some(raw) = (unsafe { frame.as_ref() }) else {
        return;
    };
    let Some(host) = crate::host() else {
        return;
    };
    // SAFETY: `user` is the leaked `Drawer<F>` this function was registered
    // with.
    let drawer = unsafe { &*user.cast::<Drawer<F>>() };
    let screen = match raw.screen {
        UiFrame::IN_GAME => Screen::InGame,
        UiFrame::LOADING => Screen::Loading,
        _ => Screen::Menu,
    };
    crate::host::guard(
        "panic in a frame callback",
        || (),
        || {
            let Ok(mut draw) = drawer.draw.lock() else {
                return;
            };
            (draw)(&mut Frame {
                host,
                token: raw.token,
                screen,
                _life: PhantomData,
                _thread: PhantomData,
            });
        },
    );
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

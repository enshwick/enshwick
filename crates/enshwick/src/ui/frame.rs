//! A frame of the mod's window, and drawing it each frame.

use core::ffi::c_void;
use core::marker::PhantomData;
use std::sync::Mutex;

use crate::Host;
use crate::abi::{Str, UiFrame, UiWidget};
use crate::locks::Relock;
use crate::ui::{Button, Dropdown, Label, MAX_OPTIONS, Screen, Slider, Sound, Toggle};

use super::widgets::read_only_flag;

/// One frame of the interface, handed to a callback of
/// [`Load::on_frame`](crate::Load::on_frame): draw widgets in order through it. Valid for the
/// callback only, on its thread.
///
/// # Examples
///
/// See the [module](crate::ui).
#[derive(Debug)]
pub struct Frame<'f> {
    pub(super) host: Host,
    pub(super) token: u64,
    pub(super) screen: Screen,
    pub(super) _life: PhantomData<&'f mut ()>,
    pub(super) _thread: PhantomData<*const ()>,
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
    /// See the [module](crate::ui).
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
pub(super) struct Drawer<F> {
    pub(super) draw: Mutex<F>,
}

pub(super) unsafe extern "system" fn draw_frame<F>(frame: *const UiFrame, user: *mut c_void)
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
            let mut draw = drawer.draw.relock();
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

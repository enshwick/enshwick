//! A frame of the mod's window, and drawing it each frame.

use core::ffi::c_void;
use core::marker::PhantomData;
use std::sync::Mutex;

use crate::abi::{Str, UiFrame, UiWidget};
use crate::locks::Relock;
use crate::sound::SoundContainerId;
use crate::ui::{Button, Dropdown, Label, MAX_OPTIONS, Screen, Slider, Sound, Toggle};
use crate::{Host, Off};

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
    /// The loader's frame, its fields past an older loader's size zero:
    /// the game screen's name, kept for the process, the prompt's title,
    /// borrowed for the callback, and the screen's host.
    pub(super) raw: UiFrame,
    pub(super) _life: PhantomData<&'f mut ()>,
    pub(super) _thread: PhantomData<*const ()>,
}

/// The size of the first `UiFrame`, which every loader passes.
const BORN: usize = core::mem::offset_of!(UiFrame, hud_screen);

/// What the player's interaction prompt offers in a frame, from
/// [`Frame::prompt`]. The name is enshwick's.
///
/// # Examples
///
/// See [`Frame::prompt`].
#[cfg(feature = "unstable")]
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct Prompt {
    /// What the prompt names, such as a chest's or a survivor's name, as
    /// the game shows it in the player's language.
    pub title: String,
    /// The game's `LocaTagId` of its verb, such as the one for "Open".
    pub verb: u32,
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

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// The game's screen the player has open in this frame, such as a
    /// storage chest's, and [`HudScreenType::InGame`](super::HudScreenType::InGame)
    /// in the world with none open; `None` outside a game, for a screen this
    /// enum does not name and on a game build where the loader does not
    /// find it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::ui::HudScreenType;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_frame(|frame| {
    ///         if frame.open_screen() == Some(HudScreenType::Map) {
    ///             // Draw beside the map.
    ///         }
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn open_screen(&self) -> Option<super::HudScreenType> {
        super::HudScreenType::from_name(self.hud_screen_name())
    }

    /// The game screen's name, kept by the loader for the process
    /// (`UiFrame`'s `hud_screen`); empty for none.
    #[cfg(feature = "unstable")]
    pub(super) fn hud_screen_name(&self) -> &'static str {
        // SAFETY: the loader keeps the text for the process, and a frame of
        // an older loader reads it zero, which is empty.
        unsafe { self.raw.hud_screen.as_str() }.unwrap_or("")
    }

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// What the player's interaction prompt offers in this frame: the name
    /// the game shows, such as a chest's, and its verb; `None` while it
    /// offers nothing, outside a game and on a game build where the loader
    /// does not find it. The prompt is the one the client's last step
    /// wrote. Allocates the title only when called.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_frame(|frame| {
    ///         if let Some(prompt) = frame.prompt() {
    ///             frame.label(format!("Looking at {}", prompt.title).as_str());
    ///         }
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn prompt(&self) -> Option<Prompt> {
        // SAFETY: the loader borrows the title for the callback, inside
        // which `self` lives; a frame of an older loader reads it zero.
        let title = unsafe { self.raw.prompt_title.as_str() }?;
        (!title.is_empty() || self.raw.prompt_verb != 0).then(|| Prompt {
            title: title.to_owned(),
            verb: self.raw.prompt_verb,
        })
    }

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// The entity the open screen belongs to, such as the chest whose
    /// inventory shows or the survivor whose dialog is open; `None` for a
    /// screen of no object, such as the map or a build tool, outside a game
    /// and on a game build where the loader does not find it. The id names
    /// the client's own world: a server's entity of the same object has
    /// another id.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_frame(|frame| {
    ///         if let Some(host) = frame.screen_host() {
    ///             frame.label(format!("Screen of {host:?}").as_str());
    ///         }
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn screen_host(&self) -> Option<crate::Entity> {
        (self.raw.screen_host.0 != 0).then_some(self.raw.screen_host)
    }

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// The template the open screen's host was made from, such as a mod's
    /// own survivor or altar kind by its content id's GUID, as the client's
    /// last step read it; `None` for no host, in the first frame of a new
    /// host, and on a game build where the loader does not find it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::id;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_frame(|frame| {
    ///         if frame.screen_host_template() == Some(id!("safe").guid()) {
    ///             frame.label("A safe");
    ///         }
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn screen_host_template(&self) -> Option<crate::content::Guid> {
        let template = self.raw.screen_host_template;
        (template != [0; 16]).then_some(crate::content::Guid(template))
    }

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Where the open screen's host stands in the client's world, in metres
    /// with y up, as the client's last step read it, so a panel's action
    /// can name the object with
    /// [`ActionTarget::at`](crate::actions::ActionTarget::at); `None`
    /// whenever [`Frame::screen_host_template`] is `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_frame(|frame| {
    ///         if let Some([x, _, z]) = frame.screen_host_position() {
    ///             frame.label(format!("At {x:.0}, {z:.0}").as_str());
    ///         }
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn screen_host_position(&self) -> Option<[f64; 3]> {
        self.screen_host_template()
            .map(|_| crate::ecs::metres(self.raw.screen_host_position))
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
    /// # extern crate enshwick_core as enshwick;
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

    /// Plays the sound container `sound`, the game's own or a mod's, as an
    /// interface sound: heard only on this client, from no place in the
    /// world.
    ///
    /// # Errors
    ///
    /// [`Off`] for a container neither the game's archive nor a mod holds,
    /// and on a loader older than ABI 11.14.
    ///
    /// # Examples
    ///
    /// See the [`sound`](crate::sound) module.
    pub fn play(&mut self, sound: SoundContainerId) -> Result<(), Off> {
        let mut widget = UiWidget {
            kind: UiWidget::PLAY,
            sound: sound.guid().0,
            ..UiWidget::EMPTY
        };
        let status = (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut widget);
        crate::load::ok(status, (), || format!("sound container {}", sound.guid()))
    }

    /// Unstable: needs the `typed` feature and may change in any release.
    ///
    /// Draws the key or button the player bound to `action`, as the game
    /// draws it beside its own prompts, with `label` beside it (empty for
    /// none); it follows the player's rebinds. True when the game drew a
    /// key, false when the action has no binding. Client only, inside a
    /// window or a screen extension.
    ///
    /// # Errors
    ///
    /// [`Off`] for an action this build does not name, outside a window, in
    /// a HUD callback, on a game build where the loader does not find the
    /// game's hint and on a loader without it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::typed::input_mapping::runtime::GameplayAction;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_frame(|frame| {
    ///         frame.window("help", "Help", |frame| {
    ///             let _shown = frame.input_hint(&GameplayAction::ContextualAction, "Open");
    ///         });
    ///     })
    /// }
    /// ```
    #[cfg(feature = "typed")]
    pub fn input_hint(
        &mut self,
        action: &crate::typed::input_mapping::runtime::GameplayAction,
        label: &str,
    ) -> Result<bool, Off> {
        use crate::typed::Reflected as _;
        let crate::resources::Value::Name(name) = action.to_value() else {
            let unnamed = crate::abi::Status::MISSING;
            return crate::load::ok(unnamed, false, || format!("game action {action:?}"));
        };
        let mut widget = UiWidget {
            kind: UiWidget::INPUT_HINT,
            text: Str::new(&name),
            detail: Str::new(label),
            ..UiWidget::EMPTY
        };
        let status = (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut widget);
        crate::load::ok(status, widget.result != 0, || format!("game action {name}"))
    }
}

/// A frame callback as the loader keeps it, with the host of the mod that
/// registered it: in the loader's own copy of the API, which runs every
/// data pack, the process's host is the loader's and not the pack's.
pub(super) struct Drawer<F> {
    pub(super) host: Host,
    pub(super) draw: Mutex<F>,
}

pub(super) unsafe extern "system" fn draw_frame<F>(frame: *const UiFrame, user: *mut c_void)
where
    F: FnMut(&mut Frame<'_>),
{
    // SAFETY: the loader passes a frame readable for the size its header
    // states, for the call; the fields an older loader lacks read zero.
    let Some(raw) = (unsafe { crate::abi::read_headed(frame, BORN) }) else {
        return;
    };
    // SAFETY: `user` is the leaked `Drawer<F>` this function was registered
    // with.
    let drawer = unsafe { &*user.cast::<Drawer<F>>() };
    let host = drawer.host;
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
                raw,
                _life: PhantomData,
                _thread: PhantomData,
            });
        },
    );
}

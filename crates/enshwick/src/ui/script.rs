//! The interface as a script draws it: the namespace `ui`.
//!
//! A [`Frame`] and a [`Hud`] are objects lent for one callback, the same
//! object on every frame. A widget, a dialog and a marker are tables of
//! their builder's fields. A toggle, slider or dropdown takes its value and
//! returns it, the player's change included, as Lua has no `&mut`.

use super::frame::Frame;
use super::hud::Hud;
use super::{
    Anchor, Button, Dialog, Dropdown, Label, Response, Screen, Slider, Sound, Style, Toggle, Ui,
};
use crate::script::{Bound, Class, Function, Object};
use crate::{Host, Load, Off};

/// A line of text as a script draws it: the fields of [`Label`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Label")]
pub(crate) struct ScriptLabel {
    /// The text.
    pub text: String,
    /// One of the game's text styles, as [`Label::style`] sets it.
    #[script(default)]
    pub style: Style,
    /// Whether it wraps at the width of its window, as [`Label::wrap`] sets it.
    #[script(default)]
    pub wrap: bool,
    /// Whether it is centred, as [`Label::center`] sets it.
    #[script(default)]
    pub center: bool,
}

impl ScriptLabel {
    fn label(&self) -> Label<'_> {
        let mut label = Label::new(&self.text).style(self.style);
        if self.wrap {
            label = label.wrap();
        }
        if self.center {
            label = label.center();
        }
        label
    }
}

/// A button as a script draws it: the fields of [`Button`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Button")]
pub(crate) struct ScriptButton {
    /// The text on it.
    pub text: String,
    /// Whether it takes clicks, as [`Button::enabled`] sets it; nil is true.
    pub enabled: Option<bool>,
}

/// A toggle row as a script draws it: the fields of [`Toggle`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Toggle")]
pub(crate) struct ScriptToggle {
    /// The row's label.
    pub label: String,
    /// Whether it shows the value without taking changes, as
    /// [`Toggle::read_only`] sets it.
    #[script(default)]
    pub read_only: bool,
}

/// A slider row as a script draws it: the fields of [`Slider`]'s builder,
/// its range as `min` and `max`.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Slider")]
pub(crate) struct ScriptSlider {
    /// The row's label.
    pub label: String,
    /// One end of the range.
    pub min: f32,
    /// The other end of the range.
    pub max: f32,
    /// Evenly spaced stops, both ends included, as [`Slider::steps`] sets
    /// them; 0 is continuous.
    #[script(default)]
    pub steps: u32,
    /// Whether it shows the value without taking changes, as
    /// [`Slider::read_only`] sets it.
    #[script(default)]
    pub read_only: bool,
}

/// A dropdown row as a script draws it: the fields of [`Dropdown`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Dropdown")]
pub(crate) struct ScriptDropdown {
    /// The row's label.
    pub label: String,
    /// The options, at most [`MAX_OPTIONS`](super::MAX_OPTIONS) of them shown.
    pub options: Vec<String>,
    /// Whether it shows the choice without taking changes, as
    /// [`Dropdown::read_only`] sets it.
    #[script(default)]
    pub read_only: bool,
}

/// A dialog as a script shows it: the fields of [`Dialog`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Dialog")]
pub(crate) struct ScriptDialog {
    /// The title.
    pub title: String,
    /// The message.
    pub message: String,
    /// Smaller text under the message, as [`Dialog::detail`] sets it.
    #[script(default)]
    pub detail: String,
    /// The buttons, as [`Dialog::buttons`] sets them; none keeps OK.
    #[script(default)]
    pub buttons: Vec<Response>,
}

/// One frame of the interface as a script draws it, lent to the callback
/// of `on_frame` or `extend_screen`: [`Frame`]'s widgets as methods.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScriptFrame {
    host: Host,
    token: u64,
    screen: Screen,
    hud_screen: crate::abi::Str,
}

impl ScriptFrame {
    fn of(frame: &Frame<'_>) -> Self {
        Self {
            host: frame.host,
            token: frame.token,
            screen: frame.screen,
            hud_screen: frame.hud_screen,
        }
    }

    /// The frame to draw through; the loader refuses its widgets once the
    /// frame is over or on another thread.
    fn frame(&self) -> Frame<'_> {
        Frame {
            host: self.host,
            token: self.token,
            screen: self.screen,
            hud_screen: self.hud_screen,
            _life: core::marker::PhantomData,
            _thread: core::marker::PhantomData,
        }
    }
}

impl Object for ScriptFrame {
    const CLASS: &'static Class = &Class::lent::<Self>(
        "Frame",
        "One frame of the interface: draw widgets in order through it, during its callback.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[
        Self::SCRIPT_FRAME,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_OPEN_SCREEN,
    ];
}

#[cfg(feature = "unstable")]
#[crate::script(open_screen)]
impl ScriptFrame {
    /// The game's screen the player has open in this frame, as
    /// [`Frame::open_screen`] says.
    #[script(name = "open_screen")]
    pub(crate) fn open_screen_script(&self) -> Option<super::HudScreenType> {
        self.frame().open_screen()
    }
}

#[crate::script(frame)]
impl ScriptFrame {
    /// Where the player is, as [`Frame::screen`] says.
    #[script(name = "screen")]
    pub(crate) const fn screen_script(&self) -> Screen {
        self.screen
    }

    /// Draws a window titled `title` and its contents with `body`, which
    /// gets this frame, as [`Frame::window`] does; false when it could not
    /// be drawn, and `body` did not run.
    #[script(name = "window")]
    pub(crate) fn window_script(
        &self,
        id: &str,
        title: &str,
        body: impl Fn(Self) + Send + Sync + 'static,
    ) -> bool {
        self.frame().window(id, title, |_| body(*self))
    }

    /// Draws a line of text, as [`Frame::label`] does.
    #[script(name = "label")]
    pub(crate) fn label_script(&self, label: &ScriptLabel) {
        self.frame().label(label.label());
    }

    /// Draws a button, as [`Frame::button`] does; true in the frame it is
    /// clicked.
    #[script(name = "button")]
    pub(crate) fn button_script(&self, button: &ScriptButton) -> bool {
        self.frame()
            .button(Button::new(&button.text).enabled(button.enabled.unwrap_or(true)))
    }

    /// Draws a toggle row showing `on`, as [`Frame::toggle`] does, and
    /// returns the value, changed in the frame the player changes it.
    #[script(name = "toggle")]
    pub(crate) fn toggle_script(&self, toggle: &ScriptToggle, on: bool) -> bool {
        let mut on = on;
        let _changed = self.frame().toggle(
            Toggle::new(&toggle.label).read_only(toggle.read_only),
            &mut on,
        );
        on
    }

    /// Draws a slider row showing `value`, as [`Frame::slider`] does, and
    /// returns the value, changed in the frame the player changes it.
    #[script(name = "slider")]
    pub(crate) fn slider_script(&self, slider: &ScriptSlider, value: f32) -> f32 {
        let mut value = value;
        let _changed = self.frame().slider(
            Slider::new(&slider.label, slider.min..=slider.max)
                .steps(slider.steps)
                .read_only(slider.read_only),
            &mut value,
        );
        value
    }

    /// Draws a dropdown row showing option `index`, from 0, as
    /// [`Frame::dropdown`] does, and returns the index, changed in the
    /// frame the player picks another.
    #[script(name = "dropdown")]
    pub(crate) fn dropdown_script(&self, dropdown: &ScriptDropdown, index: usize) -> usize {
        let options: Vec<&str> = dropdown.options.iter().map(String::as_str).collect();
        let mut index = index;
        let _changed = self.frame().dropdown(
            Dropdown::new(&dropdown.label, &options).read_only(dropdown.read_only),
            &mut index,
        );
        index
    }

    /// Draws a bar filled to `fraction`, from 0 to 1, as
    /// [`Frame::progress`] does.
    #[script(name = "progress")]
    pub(crate) fn progress_script(&self, fraction: f32) {
        self.frame().progress(fraction);
    }

    /// Plays one of the game's interface sounds, as [`Frame::sound`] does.
    #[script(name = "sound")]
    pub(crate) fn sound_script(&self, sound: Sound) {
        self.frame().sound(sound);
    }
}

/// The HUD of one frame as a script draws it, lent to the callback of
/// `on_hud`: [`Hud`]'s lines and bars as methods.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScriptHud {
    host: Host,
    token: u64,
}

impl ScriptHud {
    fn hud(&self) -> Hud<'_> {
        Hud {
            host: self.host,
            token: self.token,
            _life: core::marker::PhantomData,
            _thread: core::marker::PhantomData,
        }
    }
}

impl Object for ScriptHud {
    const CLASS: &'static Class = &Class::lent::<Self>(
        "Hud",
        "The HUD of one frame: what it draws shows this frame only, during its callback.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_HUD];
}

#[crate::script(hud)]
impl ScriptHud {
    /// Draws a line of text at `anchor`, as [`Hud::text`] does.
    #[script(name = "text")]
    pub(crate) fn text_script(&self, anchor: Anchor, label: &ScriptLabel) {
        self.hud().text(anchor, label.label());
    }

    /// Draws a bar filled to `fraction`, from 0 to 1, at `anchor`, as
    /// [`Hud::progress`] does.
    #[script(name = "progress")]
    pub(crate) fn progress_script(&self, anchor: Anchor, fraction: f32) {
        self.hud().progress(anchor, fraction);
    }
}

#[crate::script(ui)]
impl Ui {
    /// Queues `dialog` and runs `on_answer` with the player's answer, nil
    /// when it closed without one, as [`Ui::dialog`] does.
    ///
    /// # Errors
    ///
    /// [`Off`] with the [`UiError`](super::UiError) of [`Ui::dialog`].
    #[script(name = "dialog")]
    pub(crate) fn dialog_script(
        &self,
        dialog: &ScriptDialog,
        on_answer: impl Fn(Option<Response>) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let shown = Dialog::new(&dialog.title, &dialog.message)
            .detail(&dialog.detail)
            .buttons(&dialog.buttons);
        self.dialog(&shown, on_answer)
            .map_err(|e| Off::new(format!("dialog: {e}")))
    }
}

impl Bound for Ui {
    const NAME: &'static str = "ui";
    const DOC: &'static str = "The client's interface, drawn with the game's own widgets.";
    const PARTS: &'static [&'static [Function<Self>]] = &[
        Self::SCRIPT_UI,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_MARKERS,
    ];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.ui()
    }
}

#[crate::script(ui)]
impl Load<'_> {
    /// Runs `draw` once per frame of the interface with the frame lent to
    /// it, as [`Load::on_frame`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::on_frame`].
    #[script(name = "on_frame")]
    pub(crate) fn on_frame_script(
        &mut self,
        draw: impl Fn(ScriptFrame) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_frame(move |frame| draw(ScriptFrame::of(frame)))
    }

    /// Runs `draw` once per frame while the game draws its HUD, with the
    /// HUD lent to it, as [`Load::on_hud`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::on_hud`].
    #[script(name = "on_hud")]
    pub(crate) fn on_hud_script(
        &mut self,
        draw: impl Fn(ScriptHud) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_hud(move |hud| {
            draw(ScriptHud {
                host: hud.host,
                token: hud.token,
            });
        })
    }
}

#[cfg(feature = "unstable")]
mod unstable {
    use super::{ScriptFrame, Ui};
    use crate::map::{MapMarkerType, MapMarkerTypeId};
    use crate::ui::{Edge, HudScreenType, Marker, MarkerColor};
    use crate::{Load, Off};

    /// A marker as a script shows it: the fields of [`Marker`]'s builder.
    #[derive(Clone, Debug, crate::Script)]
    #[script(name = "Marker")]
    pub(crate) struct ScriptMarker {
        /// Where, in metres, y up.
        pub position: [f64; 3],
        /// The label beside its icon, as [`Marker::label`] sets it.
        #[script(default)]
        pub label: String,
        /// Its colour, as [`Marker::color`] sets it.
        #[script(default)]
        pub color: MarkerColor,
        /// The game's marker type, as [`Marker::marker_type`] sets it.
        pub marker_type: Option<MapMarkerTypeId>,
    }

    #[crate::script(markers)]
    impl Ui {
        /// Shows `markers` on the map in place of the mod's markers so far,
        /// as [`Ui::set_markers`] does.
        ///
        /// # Errors
        ///
        /// [`Off`] with the [`UiError`](crate::ui::UiError) of
        /// [`Ui::set_markers`].
        #[script(name = "set_markers")]
        pub(crate) fn set_markers_script(&self, markers: &[ScriptMarker]) -> Result<(), Off> {
            let shown: Vec<Marker<'_>> = markers
                .iter()
                .map(|m| {
                    let marker = Marker::new(m.position).label(&m.label).color(m.color);
                    match m.marker_type {
                        Some(ty) => marker.marker_type(ty),
                        None => marker,
                    }
                })
                .collect();
            self.set_markers(&shown)
                .map_err(|e| Off::new(format!("markers: {e}")))
        }

        /// Takes the mod's markers off the map, as [`Ui::clear_markers`]
        /// does.
        ///
        /// # Errors
        ///
        /// As [`Ui::clear_markers`].
        #[script(name = "clear_markers")]
        pub(crate) fn clear_markers_script(&self) -> Result<(), Off> {
            self.clear_markers()
                .map_err(|e| Off::new(format!("markers: {e}")))
        }

        /// Every marker type of the game's registry, as
        /// [`Ui::map_marker_types`] reads them.
        ///
        /// # Errors
        ///
        /// As [`Ui::map_marker_types`].
        #[script(name = "map_marker_types")]
        pub(crate) fn map_marker_types_script(&self) -> Result<Vec<MapMarkerType>, Off> {
            self.map_marker_types()
                .map_err(|e| Off::new(format!("marker types: {e}")))
        }
    }

    #[crate::script(ui_extension)]
    impl Load<'_> {
        /// Adds a panel at `edge` of the game's screen `screen` and runs
        /// `draw` once per frame while it is open, with the frame lent to
        /// it, as [`Load::extend_screen`] does.
        ///
        /// # Errors
        ///
        /// As [`Load::extend_screen`].
        #[script(name = "extend_screen")]
        pub(crate) fn extend_screen_script(
            &mut self,
            screen: HudScreenType,
            edge: Edge,
            draw: impl Fn(ScriptFrame) + Send + Sync + 'static,
        ) -> Result<(), Off> {
            self.extend_screen(screen, edge, move |frame| draw(ScriptFrame::of(frame)))
        }

        /// Calls `handler` with each change of the game's screen the player
        /// has open, as [`Load::on_screen_changed`] does.
        ///
        /// # Errors
        ///
        /// As [`Load::on_screen_changed`].
        #[script(name = "on_screen_changed")]
        pub(crate) fn on_screen_changed_script(
            &mut self,
            handler: impl Fn(crate::ui::ScreenChanged) + Send + Sync + 'static,
        ) -> Result<(), Off> {
            self.on_screen_changed(move |change| handler(*change))
        }
    }
}

//! The rows a frame draws: labels, buttons, toggles, sliders and dropdowns,
//! with their styles and sounds.

use core::ops::RangeInclusive;

use crate::abi::UiWidget;

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
    pub(super) fn to_raw(self) -> u32 {
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
    pub(super) fn to_raw(self) -> u32 {
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
    pub(super) text: &'a str,
    pub(super) style: Style,
    pub(super) flags: u32,
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
    pub(super) text: &'a str,
    pub(super) enabled: bool,
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
    pub(super) label: &'a str,
    pub(super) read_only: bool,
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
    pub(super) label: &'a str,
    pub(super) range: RangeInclusive<f32>,
    pub(super) steps: u32,
    pub(super) read_only: bool,
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
    pub(super) label: &'a str,
    pub(super) options: &'a [&'a str],
    pub(super) read_only: bool,
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

pub(super) const fn read_only_flag(read_only: bool) -> u32 {
    if read_only { UiWidget::READ_ONLY } else { 0 }
}

//! Markers on the client's map.

use crate::abi::{Header, Status, Str, UiMarker};

use super::{Ui, UiError};

/// The most markers a mod shows at once.
pub const MAX_MARKERS: usize = 256;

/// One of the game's custom marker colours.
///
/// # Examples
///
/// See [`Marker`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MarkerColor {
    /// White.
    #[default]
    White,
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// Yellow.
    Yellow,
    /// Purple.
    Purple,
    /// Turquoise.
    Turquoise,
    /// Black.
    Black,
}

impl MarkerColor {
    fn to_raw(self) -> u32 {
        match self {
            Self::White => UiMarker::WHITE,
            Self::Red => UiMarker::RED,
            Self::Green => UiMarker::GREEN,
            Self::Blue => UiMarker::BLUE,
            Self::Yellow => UiMarker::YELLOW,
            Self::Purple => UiMarker::PURPLE,
            Self::Turquoise => UiMarker::TURQUOISE,
            Self::Black => UiMarker::BLACK,
        }
    }
}

/// A marker on the map: where, its label and its colour.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Marker, MarkerColor, Ui};
///
/// fn mark(ui: Ui) {
///     let locker = Marker::new([120.0, 41.5, -36.0])
///         .label("Locker of Bob")
///         .color(MarkerColor::Green);
///     let _shown = ui.set_markers(&[locker]);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker<'a> {
    position: [f64; 3],
    label: &'a str,
    color: MarkerColor,
}

impl<'a> Marker<'a> {
    /// A white marker without a label at `position`, in metres, y up, as
    /// [`World::position`](crate::world::World::position) gives it.
    ///
    /// # Examples
    ///
    /// See [`Marker`].
    #[must_use]
    pub const fn new(position: [f64; 3]) -> Self {
        Self {
            position,
            label: "",
            color: MarkerColor::White,
        }
    }

    /// With `label` beside its icon.
    ///
    /// # Examples
    ///
    /// See [`Marker`].
    #[must_use]
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// In `color`.
    ///
    /// # Examples
    ///
    /// See [`Marker`].
    #[must_use]
    pub const fn color(mut self, color: MarkerColor) -> Self {
        self.color = color;
        self
    }
}

impl Ui {
    /// Shows `markers` on the map in place of the mod's markers so far; an
    /// empty slice clears them. The loader copies them; any thread.
    ///
    /// # Errors
    ///
    /// [`UiError::Refused`] past [`MAX_MARKERS`].
    ///
    /// # Examples
    ///
    /// See [`Marker`].
    pub fn set_markers(&self, markers: &[Marker<'_>]) -> Result<(), UiError> {
        let api = self.host.api();
        if markers.len() > MAX_MARKERS {
            return Err(UiError::Refused);
        }
        let raw: Vec<UiMarker> = markers
            .iter()
            .map(|m| UiMarker {
                header: Header::of::<UiMarker>(),
                position: crate::ecs::fixed(m.position),
                label: Str::new(m.label),
                color: m.color.to_raw(),
                reserved: 0,
            })
            .collect();
        let count = u32::try_from(raw.len()).map_err(|_| UiError::Refused)?;
        match (api.ui_markers)(self.host.handle(), raw.as_ptr(), count) {
            Status::OK => Ok(()),
            Status::MISSING => Err(UiError::Unavailable),
            _ => Err(UiError::Refused),
        }
    }

    /// Takes the mod's markers off the map.
    ///
    /// # Errors
    ///
    /// As for [`Ui::set_markers`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn unmark(ui: enshwick::ui::Ui) {
    ///     let _cleared = ui.clear_markers();
    /// }
    /// ```
    pub fn clear_markers(&self) -> Result<(), UiError> {
        self.set_markers(&[])
    }
}

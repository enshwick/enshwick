//! Markers on the client's map.

use crate::abi::{Header, MapMarkerTypeInfo, Status, Str, UiMarker};
use crate::map::{MapMarkerType, MapMarkerTypeId};

use super::{Ui, UiError};

/// The most markers a mod shows at once.
///
/// Unstable: needs the `unstable` feature and may change in any release.
pub const MAX_MARKERS: usize = super::MARKER_LIMIT;

/// One of the game's custom marker colours.
///
/// Unstable: needs the `unstable` feature and may change in any release.
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
/// Unstable: needs the `unstable` feature and may change in any release.
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
    type_id: Option<MapMarkerTypeId>,
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
            type_id: None,
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

    /// In `color`, which the game shows on its custom marker types only.
    ///
    /// # Examples
    ///
    /// See [`Marker`].
    #[must_use]
    pub const fn color(mut self, color: MarkerColor) -> Self {
        self.color = color;
        self
    }

    /// With the game's marker type `marker_type`, one of
    /// [`Ui::map_marker_types`], for the icon and the rules of that type;
    /// without it the loader picks a custom marker type with a label.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::{Marker, Ui};
    ///
    /// fn mark_altars(ui: Ui, altars: &[[f64; 3]]) {
    ///     let Some(altar) = ui
    ///         .map_marker_types()
    ///         .unwrap_or_default()
    ///         .into_iter()
    ///         .find(|ty| ty.is_fast_travel_destination)
    ///     else {
    ///         return;
    ///     };
    ///     let markers: Vec<Marker<'_>> = altars
    ///         .iter()
    ///         .map(|&at| Marker::new(at).marker_type(altar.marker_type))
    ///         .collect();
    ///     let _shown = ui.set_markers(&markers);
    /// }
    /// ```
    #[must_use]
    pub const fn marker_type(mut self, marker_type: MapMarkerTypeId) -> Self {
        self.type_id = Some(marker_type);
        self
    }
}

impl Ui {
    /// Shows `markers` on the map in place of the mod's markers so far; an
    /// empty slice clears them. The loader copies them; any thread.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`UiError::Refused`] past [`MAX_MARKERS`] or for a marker type the
    /// registry does not list; [`UiError::Unavailable`] for a marker with a
    /// type where the registry does not read. The mod's markers stay as
    /// they were.
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
                marker_type: m.type_id.map_or(0, MapMarkerTypeId::get),
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
    /// Unstable: needs the `unstable` feature and may change in any release.
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

    /// Every marker type of the game's marker type registry, in the
    /// registry's order: an open set the game's data defines. The loader
    /// reads it from the game's archive once, on the thread of the first
    /// call or of the first marker with a type, which costs that call a few
    /// milliseconds; any thread.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`UiError::Unavailable`] when the registry does not read on this
    /// game build.
    ///
    /// # Examples
    ///
    /// See [`MapMarkerType`].
    pub fn map_marker_types(&self) -> Result<Vec<MapMarkerType>, UiError> {
        let api = self.host.api();
        let size = u32::try_from(size_of::<MapMarkerTypeInfo>()).map_err(|_| UiError::Refused)?;
        let mut capacity = 0u32;
        let infos = loop {
            let mut infos = vec![MapMarkerTypeInfo::EMPTY; capacity as usize];
            let mut count = 0u32;
            let status = (api.map_marker_types)(
                self.host.handle(),
                infos.as_mut_ptr(),
                size,
                capacity,
                &raw mut count,
            );
            match status {
                Status::OK => {}
                Status::MISSING => return Err(UiError::Unavailable),
                _ => return Err(UiError::Refused),
            }
            if count <= capacity {
                infos.truncate(count as usize);
                break infos;
            }
            capacity = count;
        };
        Ok(infos.iter().map(MapMarkerType::from_info).collect())
    }
}

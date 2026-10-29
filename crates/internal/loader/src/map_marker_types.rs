//! The client's map marker types, read from the game's archive.
//!
//! The marker type registry resource holds a blob array of marker types,
//! each with its key, name, sorting and flags at offsets from the game's
//! reflection.

use core::num::NonZeroU32;

use enshwick::abi::{Header, MapMarkerTypeInfo, MarkerTypeSorting};

use crate::scene_markers::array;

/// Where the reflected types keep what the parse reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypesLayout {
    /// The registry's blob array of `MapMarkerType`.
    pub types: usize,
    /// The size of a `MapMarkerRegistryResource`.
    pub registry_size: usize,
    /// The size of a `MapMarkerType`, the array's stride.
    pub type_size: usize,
    /// `typeName`, a 16-byte text tag.
    pub type_name: usize,
    /// `markerId`, a `u32`.
    pub marker_id: usize,
    /// `sortingCategory`, a byte.
    pub sorting_category: usize,
    /// `isFastTravelDestination`, a byte.
    pub is_fast_travel_destination: usize,
    /// `isCustomMarkerType`, a byte.
    pub is_custom_marker_type: usize,
    /// `showAboveFogOfWar`, a byte.
    pub show_above_fog_of_war: usize,
    /// `showLabel`, a byte.
    pub show_label: usize,
}

impl TypesLayout {
    /// Whether the layout can be one the game has: the array header lies
    /// in the registry and every field in a marker type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::map_marker_types::TypesLayout;
    ///
    /// let layout = TypesLayout {
    ///     types: 0x10, registry_size: 0x18, type_size: 0x50, type_name: 0, marker_id: 0x10,
    ///     sorting_category: 0x14, is_fast_travel_destination: 0x18, is_custom_marker_type: 0x19,
    ///     show_above_fog_of_war: 0x40, show_label: 0x41,
    /// };
    /// assert!(layout.is_sound());
    /// assert!(!TypesLayout { show_label: 0x50, ..layout }.is_sound());
    /// ```
    #[must_use]
    pub fn is_sound(&self) -> bool {
        let fits = |at: usize, width: usize, size: usize| {
            at.checked_add(width).is_some_and(|end| end <= size)
        };
        let size = self.type_size;
        fits(self.types, 8, self.registry_size)
            && fits(self.type_name, 16, size)
            && fits(self.marker_id, 4, size)
            && [
                self.sorting_category,
                self.is_fast_travel_destination,
                self.is_custom_marker_type,
                self.show_above_fog_of_war,
                self.show_label,
            ]
            .iter()
            .all(|&at| fits(at, 1, size))
    }
}

/// Why the marker types did not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadTypesError {
    /// The reflected types the parse needs are not all there.
    #[error("the reflected marker type registry types are missing")]
    Layout,
    /// The game's archive is not open.
    #[error("the game's archive is not open")]
    Archive,
    /// The archive holds no marker type registry, as on a dedicated server.
    #[error("the archive holds no marker type registry")]
    NoRegistry,
    /// The archive holds more than one marker type registry.
    #[error("the archive holds {0} marker type registries")]
    Registries(usize),
    /// The registry is shorter than its type or its array leaves it.
    #[error("the marker type registry does not read")]
    Unreadable,
}

/// The marker types the registry resource `registry` holds, in its order,
/// with their sorting given by `groups`, the game's value of each.
///
/// # Errors
///
/// [`ReadTypesError::Layout`] for a layout that is not sound,
/// [`ReadTypesError::Unreadable`] for a resource shorter than its type or
/// an array that leaves it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::MarkerTypeSorting;
/// use enshwick_loader::map_marker_types::{TypesLayout, types};
///
/// let layout = TypesLayout {
///     types: 0x10, registry_size: 0x18, type_size: 0x50, type_name: 0, marker_id: 0x10,
///     sorting_category: 0x14, is_fast_travel_destination: 0x18, is_custom_marker_type: 0x19,
///     show_above_fog_of_war: 0x40, show_label: 0x41,
/// };
/// let mut registry = vec![0u8; 0x18 + 0x50];
/// registry[0x10..0x14].copy_from_slice(&8i32.to_le_bytes());
/// registry[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
/// registry[0x18 + 0x10..0x18 + 0x14].copy_from_slice(&0xabcdu32.to_le_bytes());
/// registry[0x18 + 0x14] = 2;
/// let read = types(&layout, &[(2, MarkerTypeSorting::DUNGEONS)], &registry)?;
/// assert_eq!(read[0].marker_id, 0xabcd);
/// assert_eq!(read[0].sorting_category, MarkerTypeSorting::DUNGEONS);
/// # Ok::<(), enshwick_loader::map_marker_types::ReadTypesError>(())
/// ```
pub fn types(
    layout: &TypesLayout,
    groups: &[(u8, MarkerTypeSorting)],
    registry: &[u8],
) -> Result<Vec<MapMarkerTypeInfo>, ReadTypesError> {
    if !layout.is_sound() {
        return Err(ReadTypesError::Layout);
    }
    if registry.len() < layout.registry_size {
        return Err(ReadTypesError::Unreadable);
    }
    let (start, count) =
        array(registry, layout.types, layout.type_size).ok_or(ReadTypesError::Unreadable)?;
    (0..count)
        .map(|i| {
            let entry = registry
                .get(start + i * layout.type_size..)
                .and_then(|rest| rest.get(..layout.type_size))
                .ok_or(ReadTypesError::Unreadable)?;
            let byte = |at: usize| entry.get(at).copied().unwrap_or(0);
            let type_name =
                bytes::<16>(entry, layout.type_name).ok_or(ReadTypesError::Unreadable)?;
            let marker_id =
                bytes::<4>(entry, layout.marker_id).ok_or(ReadTypesError::Unreadable)?;
            let raw = byte(layout.sorting_category);
            let sorting_category = groups
                .iter()
                .find(|&&(value, _)| value == raw)
                .map_or(MarkerTypeSorting::UNKNOWN, |&(_, sorting)| sorting);
            let flag = |field: usize| u8::from(byte(field) != 0);
            Ok(MapMarkerTypeInfo {
                header: Header::of::<MapMarkerTypeInfo>(),
                type_name,
                marker_id: u32::from_le_bytes(marker_id),
                sorting_category,
                is_fast_travel_destination: flag(layout.is_fast_travel_destination),
                is_custom_marker_type: flag(layout.is_custom_marker_type),
                show_above_fog_of_war: flag(layout.show_above_fog_of_war),
                show_label: flag(layout.show_label),
                reserved: [0; 3],
            })
        })
        .collect()
}

/// What the loader keeps of the marker type registry the client's map runs
/// on, which it reads from the game's memory, apart from the archive's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningTypes {
    /// The marker type for a mod marker without its own.
    pub default: u32,
    /// Every marker type the running registry lists.
    pub ids: Box<[u32]>,
}

impl RunningTypes {
    /// The marker type the map's entry gets for a marker whose mod chose
    /// `own`: that one where the running registry lists it, else the
    /// default, so only a type the running map knows reaches its list.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::num::NonZeroU32;
    /// use enshwick_loader::map_marker_types::RunningTypes;
    ///
    /// let running = RunningTypes { default: 7, ids: Box::new([7, 9]) };
    /// assert_eq!(running.choose(NonZeroU32::new(9)), 9);
    /// assert_eq!(running.choose(None), 7);
    /// ```
    #[must_use]
    pub fn choose(&self, own: Option<NonZeroU32>) -> u32 {
        own.map(NonZeroU32::get)
            .filter(|id| self.ids.contains(id))
            .unwrap_or(self.default)
    }
}

fn bytes<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of clients 426 and servers 399, from their reflection.
    const LAYOUT: TypesLayout = TypesLayout {
        types: 0x10,
        registry_size: 0x18,
        type_size: 0x50,
        type_name: 0,
        marker_id: 0x10,
        sorting_category: 0x14,
        is_fast_travel_destination: 0x18,
        is_custom_marker_type: 0x19,
        show_above_fog_of_war: 0x40,
        show_label: 0x41,
    };

    const GROUPS: [(u8, MarkerTypeSorting); 2] = [
        (1, MarkerTypeSorting::FLAME_RELATED),
        (4, MarkerTypeSorting::CUSTOM_MARKER),
    ];

    /// A registry of `entries`, each `(id, sorting, fast travel, custom)`,
    /// placed after the resource.
    fn registry(entries: &[(u32, u8, bool, bool)]) -> Vec<u8> {
        let mut data = vec![0u8; LAYOUT.registry_size + entries.len() * LAYOUT.type_size];
        let offset = i32::try_from(LAYOUT.registry_size - LAYOUT.types).unwrap();
        data[0x10..0x14].copy_from_slice(&offset.to_le_bytes());
        data[0x14..0x18].copy_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (i, &(id, sorting, fast, custom)) in entries.iter().enumerate() {
            let at = LAYOUT.registry_size + i * LAYOUT.type_size;
            data[at] = 0x11;
            data[at + 0x10..at + 0x14].copy_from_slice(&id.to_le_bytes());
            data[at + 0x14] = sorting;
            data[at + 0x18] = u8::from(fast);
            data[at + 0x19] = u8::from(custom);
            data[at + 0x41] = 7;
        }
        data
    }

    #[test]
    fn each_type_reads_its_key_name_sorting_and_flags() {
        let data = registry(&[(0xaa, 1, true, false), (0xbb, 4, false, true)]);
        let read = types(&LAYOUT, &GROUPS, &data).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].marker_id, 0xaa);
        assert_eq!(read[0].type_name[0], 0x11);
        assert_eq!(read[0].sorting_category, MarkerTypeSorting::FLAME_RELATED);
        assert_eq!(
            (
                read[0].is_fast_travel_destination,
                read[0].is_custom_marker_type
            ),
            (1, 0)
        );
        assert_eq!(read[0].show_label, 1, "any non-zero byte is set");
        assert_eq!(read[1].sorting_category, MarkerTypeSorting::CUSTOM_MARKER);
        assert_eq!(read[1].is_custom_marker_type, 1);
    }

    #[test]
    fn a_sorting_the_game_added_reads_as_unknown() {
        let data = registry(&[(0xaa, 9, false, false)]);
        let read = types(&LAYOUT, &GROUPS, &data).unwrap();
        assert_eq!(read[0].sorting_category, MarkerTypeSorting::UNKNOWN);
    }

    #[test]
    fn an_empty_registry_has_no_types() {
        let data = vec![0u8; LAYOUT.registry_size];
        assert_eq!(types(&LAYOUT, &GROUPS, &data).map(|read| read.len()), Ok(0));
    }

    #[test]
    fn a_registry_shorter_than_its_type_or_its_array_does_not_read() {
        let data = registry(&[(0xaa, 1, false, false)]);
        assert_eq!(
            types(&LAYOUT, &GROUPS, &data[..LAYOUT.registry_size - 1]).map(|read| read.len()),
            Err(ReadTypesError::Unreadable)
        );
        assert_eq!(
            types(&LAYOUT, &GROUPS, &data[..data.len() - 1]).map(|read| read.len()),
            Err(ReadTypesError::Unreadable)
        );
    }

    #[test]
    fn a_layout_whose_fields_leave_their_types_is_refused_before_any_read() {
        let data = registry(&[(0xaa, 1, false, false)]);
        let past = TypesLayout {
            type_name: 0x48,
            ..LAYOUT
        };
        assert_eq!(
            types(&past, &GROUPS, &data).err(),
            Some(ReadTypesError::Layout)
        );
        let overflowing = TypesLayout {
            types: usize::MAX,
            ..LAYOUT
        };
        assert!(!overflowing.is_sound());
    }

    #[test]
    fn a_mod_s_type_the_running_registry_lacks_falls_back_to_the_default() {
        let running = RunningTypes {
            default: 0x10,
            ids: Box::new([0x10, 0x20]),
        };
        assert_eq!(running.choose(NonZeroU32::new(0x20)), 0x20);
        assert_eq!(
            running.choose(NonZeroU32::new(0x30)),
            0x10,
            "listed only in the archive"
        );
        assert_eq!(running.choose(None), 0x10);
    }

    proptest::proptest! {
        #[test]
        fn any_bytes_read_or_refuse_without_a_panic(data in proptest::collection::vec(proptest::num::u8::ANY, 0..400)) {
            let _read = types(&LAYOUT, &GROUPS, &data);
        }
    }
}

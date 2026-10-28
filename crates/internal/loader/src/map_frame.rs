//! The map's frame, read from the scene's biome map resource in the game's
//! archive: where the world's map starts and how far it reaches.
//!
//! The scene resource says whether the scene has a biome map; the biome map
//! resource starts with the world's origin (two `i32`), its size in cells
//! (two `u32`) and the metres per cell (`u32`), at offsets from the game's
//! reflection.

/// Where the reflected types keep what the parse reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLayout {
    /// `SceneResource.hasBiomeMap`, a one-byte flag.
    pub has_biome_map: usize,
    /// The size of a `SceneResource`.
    pub scene_size: usize,
    /// `BiomeMapResource.worldOrigin`, x then z.
    pub world_origin: usize,
    /// `BiomeMapResource.worldSize`, in cells, x then z.
    pub world_size: usize,
    /// `BiomeMapResource.mapScale`, metres per cell.
    pub map_scale: usize,
    /// The size of a `BiomeMapResource`.
    pub biome_map_size: usize,
}

impl FrameLayout {
    /// Whether the layout can be one the game has: the flag lies in the
    /// scene resource and every field in the biome map resource.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::map_frame::FrameLayout;
    ///
    /// let layout = FrameLayout {
    ///     has_biome_map: 200, scene_size: 208,
    ///     world_origin: 0, world_size: 8, map_scale: 16, biome_map_size: 28,
    /// };
    /// assert!(layout.is_sound());
    /// assert!(!FrameLayout { map_scale: 26, ..layout }.is_sound());
    /// ```
    #[must_use]
    pub fn is_sound(&self) -> bool {
        let fits = |at: usize, width: usize, size: usize| {
            at.checked_add(width).is_some_and(|end| end <= size)
        };
        fits(self.has_biome_map, 1, self.scene_size)
            && fits(self.world_origin, 8, self.biome_map_size)
            && fits(self.world_size, 8, self.biome_map_size)
            && fits(self.map_scale, 4, self.biome_map_size)
    }
}

/// The frame as the biome map resource holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameData {
    /// `worldOrigin`, x then z.
    pub origin: [i32; 2],
    /// `worldSize`, in cells, x then z.
    pub cells: [u32; 2],
    /// `mapScale`, metres per cell.
    pub scale: u32,
}

impl FrameData {
    /// The frame in metres: the origin, the size and the metres per cell.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::map_frame::FrameData;
    ///
    /// let data = FrameData { origin: [0, 0], cells: [1280, 1280], scale: 8 };
    /// assert_eq!(data.metres(), ([0.0, 0.0], [10240.0, 10240.0], 8.0));
    /// ```
    #[must_use]
    pub fn metres(&self) -> ([f64; 2], [f64; 2], f64) {
        let scale = f64::from(self.scale);
        (
            self.origin.map(f64::from),
            self.cells.map(|cells| f64::from(cells) * scale),
            scale,
        )
    }
}

/// The step at which a scene's frame did not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadFrameError {
    /// The reflected types the parse needs are not all there.
    #[error("the reflected scene and biome map types are missing")]
    Layout,
    /// The read panicked.
    #[error("the read panicked")]
    Panicked,
    /// The game's archive is not open.
    #[error("the game's archive is not open")]
    Archive,
    /// The archive lacks the scene resource.
    #[error("the archive lacks the scene resource")]
    Scene,
    /// The scene resource is shorter than its type.
    #[error("the scene resource does not read")]
    SceneUnreadable,
    /// The scene says it has no biome map.
    #[error("the scene has no biome map")]
    NoBiomeMap,
    /// The archive lacks the scene's biome map.
    #[error("the archive lacks the scene's biome map")]
    BiomeMap,
    /// The biome map is shorter than its type or holds an empty frame.
    #[error("the scene's biome map does not read")]
    BiomeMapUnreadable,
    /// No thread could be started to read the scene.
    #[error("no thread could be started to read the scene")]
    Thread,
}

/// Whether the scene resource `scene` says the scene has a biome map.
///
/// # Errors
///
/// [`ReadFrameError::Layout`] for a layout that is not sound,
/// [`ReadFrameError::SceneUnreadable`] for a resource shorter than its type.
///
/// # Examples
///
/// ```
/// use enshwick_loader::map_frame::{FrameLayout, has_biome_map};
///
/// let layout = FrameLayout {
///     has_biome_map: 200, scene_size: 208,
///     world_origin: 0, world_size: 8, map_scale: 16, biome_map_size: 28,
/// };
/// let mut scene = vec![0u8; 208];
/// assert_eq!(has_biome_map(&layout, &scene), Ok(false));
/// scene[200] = 1;
/// assert_eq!(has_biome_map(&layout, &scene), Ok(true));
/// # Ok::<(), enshwick_loader::map_frame::ReadFrameError>(())
/// ```
pub fn has_biome_map(layout: &FrameLayout, scene: &[u8]) -> Result<bool, ReadFrameError> {
    if !layout.is_sound() {
        return Err(ReadFrameError::Layout);
    }
    if scene.len() < layout.scene_size {
        return Err(ReadFrameError::SceneUnreadable);
    }
    scene
        .get(layout.has_biome_map)
        .map(|&flag| flag != 0)
        .ok_or(ReadFrameError::SceneUnreadable)
}

fn bytes<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}

/// The frame the biome map resource `biome_map` holds.
///
/// # Errors
///
/// [`ReadFrameError::Layout`] for a layout that is not sound,
/// [`ReadFrameError::BiomeMapUnreadable`] for a resource shorter than its
/// type or one with no cells or no metres per cell.
///
/// # Examples
///
/// ```
/// use enshwick_loader::map_frame::{FrameData, FrameLayout, frame};
///
/// let layout = FrameLayout {
///     has_biome_map: 200, scene_size: 208,
///     world_origin: 0, world_size: 8, map_scale: 16, biome_map_size: 28,
/// };
/// let mut data = vec![0u8; 28];
/// data[8..12].copy_from_slice(&1280u32.to_le_bytes());
/// data[12..16].copy_from_slice(&1280u32.to_le_bytes());
/// data[16..20].copy_from_slice(&8u32.to_le_bytes());
/// assert_eq!(frame(&layout, &data)?, FrameData { origin: [0, 0], cells: [1280, 1280], scale: 8 });
/// # Ok::<(), enshwick_loader::map_frame::ReadFrameError>(())
/// ```
pub fn frame(layout: &FrameLayout, biome_map: &[u8]) -> Result<FrameData, ReadFrameError> {
    if !layout.is_sound() {
        return Err(ReadFrameError::Layout);
    }
    if biome_map.len() < layout.biome_map_size {
        return Err(ReadFrameError::BiomeMapUnreadable);
    }
    let read = || {
        let pair = |at: usize| Some([bytes::<4>(biome_map, at)?, bytes(biome_map, at + 4)?]);
        let [x, z] = pair(layout.world_origin)?;
        let [width, depth] = pair(layout.world_size)?;
        Some(FrameData {
            origin: [i32::from_le_bytes(x), i32::from_le_bytes(z)],
            cells: [u32::from_le_bytes(width), u32::from_le_bytes(depth)],
            scale: u32::from_le_bytes(bytes(biome_map, layout.map_scale)?),
        })
    };
    read()
        .filter(|data| data.scale > 0 && data.cells.iter().all(|&cells| cells > 0))
        .ok_or(ReadFrameError::BiomeMapUnreadable)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const LAYOUT: FrameLayout = FrameLayout {
        has_biome_map: 200,
        scene_size: 208,
        world_origin: 0,
        world_size: 8,
        map_scale: 16,
        biome_map_size: 28,
    };

    /// A biome map resource with `origin`, `cells` and `scale` and a blob
    /// of `blob` bytes after it, as the archive keeps it.
    fn biome_map(origin: [i32; 2], cells: [u32; 2], scale: u32, blob: usize) -> Vec<u8> {
        let mut data = Vec::with_capacity(28 + blob);
        data.extend(origin[0].to_le_bytes());
        data.extend(origin[1].to_le_bytes());
        data.extend(cells[0].to_le_bytes());
        data.extend(cells[1].to_le_bytes());
        data.extend(scale.to_le_bytes());
        data.extend([0u8; 8]);
        data.resize(28 + blob, 0);
        data
    }

    #[test]
    fn a_biome_map_gives_its_origin_cells_and_scale() {
        let data = biome_map([-64, 128], [1280, 960], 8, 1280 * 960);
        assert_eq!(
            frame(&LAYOUT, &data),
            Ok(FrameData {
                origin: [-64, 128],
                cells: [1280, 960],
                scale: 8
            })
        );
    }

    #[test]
    fn a_biome_map_shorter_than_its_type_does_not_read() {
        let data = biome_map([0, 0], [1280, 1280], 8, 0);
        assert_eq!(
            frame(&LAYOUT, &data[..27]),
            Err(ReadFrameError::BiomeMapUnreadable)
        );
        assert_eq!(frame(&LAYOUT, &[]), Err(ReadFrameError::BiomeMapUnreadable));
    }

    #[test]
    fn a_biome_map_with_no_cells_or_no_scale_does_not_read() {
        for data in [
            biome_map([0, 0], [0, 1280], 8, 0),
            biome_map([0, 0], [1280, 0], 8, 0),
            biome_map([0, 0], [1280, 1280], 0, 0),
        ] {
            assert_eq!(
                frame(&LAYOUT, &data),
                Err(ReadFrameError::BiomeMapUnreadable)
            );
        }
    }

    #[test]
    fn a_scene_without_a_biome_map_says_so() {
        let mut scene = vec![0u8; 208];
        assert_eq!(has_biome_map(&LAYOUT, &scene), Ok(false));
        scene[200] = 1;
        assert_eq!(has_biome_map(&LAYOUT, &scene), Ok(true));
    }

    #[test]
    fn a_scene_shorter_than_its_type_does_not_read() {
        assert_eq!(
            has_biome_map(&LAYOUT, &[1u8; 207]),
            Err(ReadFrameError::SceneUnreadable)
        );
    }

    #[test]
    fn a_layout_whose_fields_leave_their_types_is_refused_before_any_read() {
        let unsound = [
            FrameLayout {
                has_biome_map: 208,
                ..LAYOUT
            },
            FrameLayout {
                world_origin: 21,
                ..LAYOUT
            },
            FrameLayout {
                world_size: usize::MAX,
                ..LAYOUT
            },
            FrameLayout {
                map_scale: 25,
                ..LAYOUT
            },
            FrameLayout {
                biome_map_size: 19,
                ..LAYOUT
            },
        ];
        let scene = [1u8; 512];
        let data = biome_map([0, 0], [1280, 1280], 8, 0);
        for layout in unsound {
            assert!(!layout.is_sound(), "{layout:?}");
            assert_eq!(has_biome_map(&layout, &scene), Err(ReadFrameError::Layout));
            assert_eq!(frame(&layout, &data), Err(ReadFrameError::Layout));
        }
    }

    proptest! {
        #[test]
        fn any_bytes_parse_without_a_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
            let _flag = has_biome_map(&LAYOUT, &data);
            let _frame = frame(&LAYOUT, &data);
        }
    }
}

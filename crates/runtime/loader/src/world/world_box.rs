//! The box of the world a scene keeps players in, read from its scene
//! resource in the game's archive: past it the game moves a player back and
//! removes any other entity.
//!
//! The scene resource holds the box in its `resetPlayersOutOfBounds` as two
//! `WorldPosition` corners in the engine's 32.32 fixed point, at offsets
//! from the game's reflection (server 26430253: 0 to 10,240 m across, 0 to
//! 2,560 m high).

use enshwick_core::abi::Metres;

/// The bytes of one axis of a `WorldPosition`, an `i64`.
const AXIS: usize = size_of::<i64>();

/// Where the reflected types keep what the parse reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxLayout {
    /// `SceneResource.resetPlayersOutOfBounds`.
    pub bounds: usize,
    /// `SceneResetPlayersOutOfBounds.worldMin`, from `bounds`.
    pub world_min: usize,
    /// `SceneResetPlayersOutOfBounds.worldMax`, from `bounds`.
    pub world_max: usize,
    /// The size of a `SceneResource`.
    pub scene_size: usize,
    /// The size of a `WorldPosition`, one corner.
    pub corner: usize,
    /// `WorldPosition.x`, `y` and `z`, from a corner.
    pub axes: [usize; 3],
}

impl BoxLayout {
    /// Whether the layout can be one the game has: each axis lies in its
    /// corner, and both corners in the scene resource.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::world::world_box::BoxLayout;
    ///
    /// let layout = BoxLayout {
    ///     bounds: 120,
    ///     world_min: 8,
    ///     world_max: 32,
    ///     scene_size: 208,
    ///     corner: 24,
    ///     axes: [0, 8, 16],
    /// };
    /// assert!(layout.is_sound());
    /// assert!(!BoxLayout { world_max: 72, ..layout }.is_sound());
    /// assert!(!BoxLayout { axes: [0, 8, 20], ..layout }.is_sound());
    /// ```
    #[must_use]
    pub fn is_sound(&self) -> bool {
        let fits = |at: usize| {
            self.bounds
                .checked_add(at)
                .and_then(|start| start.checked_add(self.corner))
                .is_some_and(|end| end <= self.scene_size)
        };
        self.axes
            .iter()
            .all(|&axis| axis.checked_add(AXIS).is_some_and(|end| end <= self.corner))
            && fits(self.world_min)
            && fits(self.world_max)
    }
}

/// The world's box in the engine's fixed point, both corners inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldBox {
    /// The lowest corner, x, y, z.
    pub min: [i64; 3],
    /// The highest corner, x, y, z.
    pub max: [i64; 3],
}

impl WorldBox {
    /// Whether `position`, in metres, lies in the box; a position not finite
    /// or past the fixed point lies in none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::world::world_box::WorldBox;
    ///
    /// let world = WorldBox { min: [0; 3], max: [10_240 << 32, 2_560 << 32, 10_240 << 32] };
    /// assert!(world.holds([3_734.25, 843.75, 10_240.0]));
    /// assert!(!world.holds([10_300.0, 100.0, 0.0]));
    /// assert!(!world.holds([-5.0, 100.0, 0.0]));
    /// assert!(!world.holds([f64::NAN, 100.0, 0.0]));
    /// ```
    #[must_use]
    pub fn holds(&self, position: Metres) -> bool {
        enshwick_core::ecs::checked_fixed(position).is_some_and(|fixed| {
            (0..3).all(|axis| (self.min[axis]..=self.max[axis]).contains(&fixed[axis]))
        })
    }

    /// The box in metres, lowest corner first.
    #[must_use]
    pub fn metres(&self) -> (Metres, Metres) {
        (
            enshwick_core::ecs::metres(self.min),
            enshwick_core::ecs::metres(self.max),
        )
    }
}

/// The step at which a scene's box did not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadBoxError {
    /// The reflected scene types the parse needs are not all there.
    #[error("the reflected scene types are missing")]
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
    /// The scene resource is shorter than its type or holds an empty box.
    #[error("the scene's box does not read")]
    Unreadable,
    /// No thread could be started to read the scene.
    #[error("no thread could be started to read the scene")]
    Thread,
    /// A mod serves the scene resource, so the archive's box may not be the
    /// one the game keeps.
    #[error("a mod serves the scene resource")]
    Served,
}

/// The box the scene resource `scene` keeps players in.
///
/// # Errors
///
/// [`ReadBoxError::Layout`] for a layout that is not sound,
/// [`ReadBoxError::Unreadable`] for a resource shorter than its type or a
/// box whose lowest corner lies above its highest on an axis.
///
/// # Examples
///
/// ```
/// use enshwick_loader::world::world_box::{BoxLayout, WorldBox, world_box};
///
/// let layout = BoxLayout {
///     bounds: 120,
///     world_min: 8,
///     world_max: 32,
///     scene_size: 208,
///     corner: 24,
///     axes: [0, 8, 16],
/// };
/// let mut scene = vec![0u8; 208];
/// for (axis, metres) in [10_240_i64, 2_560, 10_240].into_iter().enumerate() {
///     let at = 120 + 32 + axis * 8;
///     scene[at..at + 8].copy_from_slice(&(metres << 32).to_le_bytes());
/// }
/// let world = world_box(&layout, &scene)?;
/// assert_eq!(world, WorldBox { min: [0; 3], max: [10_240 << 32, 2_560 << 32, 10_240 << 32] });
/// # Ok::<(), enshwick_loader::world::world_box::ReadBoxError>(())
/// ```
pub fn world_box(layout: &BoxLayout, scene: &[u8]) -> Result<WorldBox, ReadBoxError> {
    if !layout.is_sound() {
        return Err(ReadBoxError::Layout);
    }
    if scene.len() < layout.scene_size {
        return Err(ReadBoxError::Unreadable);
    }
    let corner = |at: usize| -> Option<[i64; 3]> {
        let start = layout.bounds + at;
        let axis = |i: usize| {
            let from = start + layout.axes[i];
            Some(i64::from_le_bytes(
                scene.get(from..from + AXIS)?.try_into().ok()?,
            ))
        };
        Some([axis(0)?, axis(1)?, axis(2)?])
    };
    let found = WorldBox {
        min: corner(layout.world_min).ok_or(ReadBoxError::Unreadable)?,
        max: corner(layout.world_max).ok_or(ReadBoxError::Unreadable)?,
    };
    (0..3)
        .all(|axis| found.min[axis] < found.max[axis])
        .then_some(found)
        .ok_or(ReadBoxError::Unreadable)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const LAYOUT: BoxLayout = BoxLayout {
        bounds: 120,
        world_min: 8,
        world_max: 32,
        scene_size: 208,
        corner: 24,
        axes: [0, 8, 16],
    };

    /// A scene resource whose box runs from `min` to `max` metres.
    fn scene(min: [i64; 3], max: [i64; 3]) -> Vec<u8> {
        let mut data = vec![0u8; 208];
        for (corner, at) in [(min, 8), (max, 32)] {
            for (axis, metres) in corner.into_iter().enumerate() {
                let from = 120 + at + axis * 8;
                data[from..from + 8].copy_from_slice(&(metres << 32).to_le_bytes());
            }
        }
        data
    }

    #[test]
    fn a_scene_gives_its_box_in_fixed_point() {
        let data = scene([0, -64, 0], [10_240, 2_560, 10_240]);
        assert_eq!(
            world_box(&LAYOUT, &data),
            Ok(WorldBox {
                min: [0, -64 << 32, 0],
                max: [10_240 << 32, 2_560 << 32, 10_240 << 32],
            })
        );
    }

    #[test]
    fn a_scene_shorter_than_its_type_or_with_an_empty_box_does_not_read() {
        let data = scene([0; 3], [10_240, 2_560, 10_240]);
        assert_eq!(
            world_box(&LAYOUT, &data[..207]),
            Err(ReadBoxError::Unreadable)
        );
        for empty in [
            scene([0; 3], [0, 2_560, 10_240]),
            scene([0; 3], [10_240, 2_560, -1]),
            scene([0; 3], [0; 3]),
        ] {
            assert_eq!(world_box(&LAYOUT, &empty), Err(ReadBoxError::Unreadable));
        }
    }

    #[test]
    fn a_layout_whose_corners_leave_the_scene_resource_is_refused_before_any_read() {
        let data = [0u8; 512];
        for unsound in [
            BoxLayout {
                world_max: 65,
                ..LAYOUT
            },
            BoxLayout {
                bounds: usize::MAX,
                ..LAYOUT
            },
            BoxLayout {
                scene_size: 175,
                ..LAYOUT
            },
            BoxLayout {
                axes: [0, 8, 17],
                ..LAYOUT
            },
            BoxLayout {
                axes: [usize::MAX, 8, 16],
                ..LAYOUT
            },
        ] {
            assert!(!unsound.is_sound(), "{unsound:?}");
            assert_eq!(world_box(&unsound, &data), Err(ReadBoxError::Layout));
        }
    }

    #[test]
    fn the_box_holds_its_corners_and_nothing_past_them_or_past_the_fixed_point() {
        let world = WorldBox {
            min: [0, -64 << 32, 0],
            max: [10_240 << 32, 2_560 << 32, 10_240 << 32],
        };
        assert!(world.holds([0.0, -64.0, 0.0]));
        assert!(world.holds([10_240.0, 2_560.0, 10_240.0]));
        for out in [
            [10_240.000_001, 0.0, 0.0],
            [0.0, 2_560.5, 0.0],
            [0.0, -64.5, 0.0],
            [0.0, 0.0, -1e-6],
            [2_147_483_647.999_999_8, 900.0, 2_147_483_647.999_999_8],
            [f64::INFINITY, 0.0, 0.0],
            [0.0, f64::NAN, 0.0],
        ] {
            assert!(!world.holds(out), "{out:?}");
        }
        assert_eq!(
            world.metres(),
            ([0.0, -64.0, 0.0], [10_240.0, 2_560.0, 10_240.0])
        );
    }

    proptest! {
        #[test]
        fn any_bytes_parse_without_a_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
            let _box = world_box(&LAYOUT, &data);
        }
    }
}

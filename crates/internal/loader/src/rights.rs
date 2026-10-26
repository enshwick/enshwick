//! Whether a player may edit a box of the server's world.
//!
//! The permissions are the game's terrain tool rule: every edit needs
//! `CanEditWorld`, and one whose box overlaps a building zone also needs
//! `CanEditBase`. The permission byte is the player's entry in
//! the server's permission table, a mask of the reflected
//! `PlayerPermissions` bit indices; bases have no owner, so the bits are
//! the player's alone. Past the permissions the loader adds two tests the
//! building cursor runs for items, in an order of the loader's own: both
//! corners inside the world's building area, then no strict no-build zone
//! the world placed. The zone tests are the game's
//! own functions on its building-zone object, which the server keeps for
//! the world's life (server 399, client 426).

use enshwick::abi::{BuildRights, Status};

use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::IN_BUILDING_AREA.id(),
    pinned::building::NO_BUILD_TEST.id(),
    pinned::building::ZONE_TOUCHES.id(),
    pinned::server::BUILD_ZONE_SYSTEM.id(),
];

/// The masks of the two permission bits the rule reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Permissions {
    /// `CanEditWorld`.
    pub(crate) edit_world: u8,
    /// `CanEditBase`.
    pub(crate) edit_base: u8,
}

impl Permissions {
    /// The masks for the bit indices `edit_world` and `edit_base`; `None`
    /// for an index past the byte.
    pub(crate) fn from_indices(edit_world: u64, edit_base: u64) -> Option<Self> {
        let bit = |index: u64| u32::try_from(index).ok().and_then(|i| 1u8.checked_shl(i));
        Some(Self {
            edit_world: bit(edit_world)?,
            edit_base: bit(edit_base)?,
        })
    }
}

/// The box of an edit in metres, checked once at the boundary: finite, and
/// `max` at least `min` on every axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditBox {
    min: [f64; 3],
    max: [f64; 3],
}

impl EditBox {
    /// The box of `check`; [`Status::BAD_ARGS`] for a corner that is not
    /// finite or a `max` below `min`.
    pub(crate) fn new(check: &BuildRights) -> Result<Self, Status> {
        let finite = check.min.iter().chain(&check.max).all(|v| v.is_finite());
        if !finite || (0..3).any(|a| check.max[a] < check.min[a]) {
            return Err(Status::BAD_ARGS);
        }
        Ok(Self {
            min: check.min,
            max: check.max,
        })
    }

    /// The box as the game's zone tests take it: min x, y, z, a pad, then
    /// max x, y, z, a pad, in metres.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the game's zone tests take metres as f32"
    )]
    pub(crate) fn to_game(self) -> [f32; 8] {
        let [min, max] = [self.min, self.max].map(|corner| corner.map(|v| v as f32));
        [min[0], min[1], min[2], 0.0, max[0], max[1], max[2], 0.0]
    }

    /// The two corners in 32.32 fixed-point metres, as the game's area
    /// test takes a point.
    pub(crate) fn corners(self) -> [[i64; 3]; 2] {
        [
            enshwick::ecs::fixed(self.min),
            enshwick::ecs::fixed(self.max),
        ]
    }
}

/// The game's answers about an edit's box the rule asks for, each asked
/// only when the rule reaches it.
pub(crate) trait Zones {
    /// Whether the box overlaps a building zone.
    fn touches_building_zone(&mut self) -> bool;
    /// Whether the point lies inside the world's building area.
    fn in_building_area(&mut self, point: [i64; 3]) -> bool;
    /// Whether the box overlaps a strict no-build zone the world placed, not a
    /// creature.
    fn in_strict_no_build_zone(&mut self) -> bool;
}

/// The refusal of an edit by a player with the permission byte `mask`, or
/// [`BuildRights::ALLOWED`]: the permissions in the terrain tool's order,
/// then the building area and the strict no-build zones, in the loader's
/// order.
pub(crate) fn refusal(
    mask: u8,
    bits: Permissions,
    corners: [[i64; 3]; 2],
    zones: &mut impl Zones,
) -> u32 {
    if mask & bits.edit_world == 0 {
        return BuildRights::NO_CAN_EDIT_WORLD;
    }
    if mask & bits.edit_base == 0 && zones.touches_building_zone() {
        return BuildRights::NO_CAN_EDIT_BASE;
    }
    if !corners.into_iter().all(|c| zones.in_building_area(c)) {
        return BuildRights::OUTSIDE_BUILDING_ZONE;
    }
    if zones.in_strict_no_build_zone() {
        return BuildRights::INSIDE_STRICT_BUILD_INHIBITOR_ZONE;
    }
    BuildRights::ALLOWED
}

#[cfg(windows)]
pub(crate) use game::check;

#[cfg(windows)]
mod game {
    use enshwick::abi::{BuildRights, Status};

    use super::{EditBox, Permissions, Zones, refusal};
    use crate::host::Loader;
    use crate::memory::read;
    use crate::pinned;

    /// `zone_touches(zones, box) -> u8`: 1 when the box overlaps a building
    /// zone with some thickness on every axis, ignored entries skipped
    /// (server 399, client 426).
    type TouchesFn = unsafe extern "system" fn(zones: *const u8, bounds: *const [f32; 8]) -> u8;
    /// `in_building_area(zones, point) -> u8`: 1 when the world has no
    /// building area or the point lies in one of its boxes (server 399,
    /// client 426).
    type AreaFn = unsafe extern "system" fn(zones: *const u8, point: *const [i64; 3]) -> u8;
    /// `no_build_test(out, zones, box, types, ignored entity)`: sets bit 1
    /// of `out` when the box overlaps a strict no-build zone of a type in
    /// `types`, bit 0 for a lenient one (server 399, client 426).
    type NoBuildFn = unsafe extern "system" fn(
        out: *mut u8,
        zones: *const u8,
        bounds: *const [f32; 8],
        types: u8,
        ignored: u32,
    ) -> *mut u8;

    /// The zone types of the moving no-build zones creatures carry around
    /// themselves: enemies 2, animals 3, NPCs 5 (server 399, client 426).
    const CREATURE_ZONES: u8 = 1 << 2 | 1 << 3 | 1 << 5;
    /// Every zone type but the creatures'.
    const PLACED_ZONES: u8 = !CREATURE_ZONES;
    /// The test's bit for a strict no-build zone (server 399, client 426).
    const STRICT: u8 = 1 << 1;

    /// The game's zone tests on the server's building-zone object.
    struct Game {
        zones: *const u8,
        bounds: [f32; 8],
        touches: TouchesFn,
        area: AreaFn,
        no_build: NoBuildFn,
    }

    impl Zones for Game {
        fn touches_building_zone(&mut self) -> bool {
            // SAFETY: `touches` is the table's `building.zone_touches`;
            // `zones` is the server's live building-zone object and
            // `bounds` eight `f32`s on this stack, called on the game thread
            // at a quiet moment, where no system changes the zone lists
            // (checked by `crate::world::server` in `check`).
            unsafe { (self.touches)(self.zones, &raw const self.bounds) != 0 }
        }

        fn in_building_area(&mut self, point: [i64; 3]) -> bool {
            // SAFETY: `area` is the table's `building.in_building_area`;
            // `zones` is the server's live building-zone object and `point`
            // three `i64`s on this stack, on the game thread at a quiet
            // moment (checked by `crate::world::server` in `check`).
            unsafe { (self.area)(self.zones, &raw const point) != 0 }
        }

        fn in_strict_no_build_zone(&mut self) -> bool {
            let mut found = 0u8;
            // SAFETY: `no_build` is the table's `building.no_build_test`;
            // `found` is one writable byte and `bounds` eight `f32`s on this
            // stack, `zones` the server's live building-zone object, on the
            // game thread at a quiet moment (checked by
            // `crate::world::server` in `check`); entity 0 skips no zone.
            let _out = unsafe {
                (self.no_build)(
                    &raw mut found,
                    self.zones,
                    &raw const self.bounds,
                    PLACED_ZONES,
                    0,
                )
            };
            found & STRICT != 0
        }
    }

    /// The refusal of `check`'s edit, or [`BuildRights::ALLOWED`]; on the
    /// game thread at a quiet moment. [`Status::MISSING`] while no world
    /// is loaded or on a build without the tests.
    pub(crate) fn check(loader: &Loader, check: &BuildRights) -> Result<u32, Status> {
        let edit = EditBox::new(check)?;
        let server = crate::world::server()?;
        let function = |id| {
            loader
                .address(id)
                .and_then(|address| usize::try_from(address).ok())
                .ok_or(Status::MISSING)
        };
        let (touches, area, no_build) = (
            function(pinned::building::ZONE_TOUCHES)?,
            function(pinned::building::IN_BUILDING_AREA)?,
            function(pinned::building::NO_BUILD_TEST)?,
        );
        let zones_at = loader
            .offset(pinned::server::BUILD_ZONE_SYSTEM)
            .ok_or(Status::MISSING)?;
        let bits = permissions(loader).ok_or(Status::MISSING)?;
        // SAFETY: `server` comes from `crate::world::server`, on the game
        // thread that alone writes it; the table's `server.build_zone_system`
        // offset holds its building-zone object, made when the world starts.
        let zones: usize = unsafe { read(server, zones_at) };
        if zones == 0 {
            return Err(Status::MISSING);
        }
        let mask = crate::world::players(loader)?
            .iter()
            .find(|player| player.entity == check.player)
            .map_or(0, |player| player.permissions);
        let mut game = Game {
            zones: core::ptr::with_exposed_provenance(zones),
            bounds: edit.to_game(),
            // SAFETY: the table's `building.zone_touches` is the address of
            // the game function with the `TouchesFn` signature on this build,
            // in the image, which stays mapped.
            touches: unsafe { core::mem::transmute::<usize, TouchesFn>(touches) },
            // SAFETY: the table's `building.in_building_area` is the address
            // of the game function with the `AreaFn` signature on this build,
            // in the image, which stays mapped.
            area: unsafe { core::mem::transmute::<usize, AreaFn>(area) },
            // SAFETY: the table's `building.no_build_test` is the address of
            // the game function with the `NoBuildFn` signature on this build,
            // in the image, which stays mapped.
            no_build: unsafe { core::mem::transmute::<usize, NoBuildFn>(no_build) },
        };
        Ok(refusal(mask, bits, edit.corners(), &mut game))
    }

    /// The masks of the permission bits, from the reflected
    /// `PlayerPermissions`.
    fn permissions(loader: &Loader) -> Option<Permissions> {
        use enshwick_gamedata::catalogue::{PERMISSION_CAN_EDIT_BASE, PERMISSION_CAN_EDIT_WORLD};
        let index = |id| crate::catalogue::constant(id, &loader.reflection);
        Permissions::from_indices(
            index(PERMISSION_CAN_EDIT_WORLD)?,
            index(PERMISSION_CAN_EDIT_BASE)?,
        )
    }
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Header;

    use super::*;

    const BITS: Permissions = Permissions {
        edit_world: 1 << 3,
        edit_base: 1 << 2,
    };
    const BOTH: u8 = BITS.edit_world | BITS.edit_base;

    /// Fixed answers, with the questions asked counted.
    #[derive(Default)]
    struct Answers {
        touches: bool,
        outside: Option<[i64; 3]>,
        strict: bool,
        asked: u32,
    }

    impl Zones for Answers {
        fn touches_building_zone(&mut self) -> bool {
            self.asked += 1;
            self.touches
        }

        fn in_building_area(&mut self, point: [i64; 3]) -> bool {
            self.asked += 1;
            self.outside != Some(point)
        }

        fn in_strict_no_build_zone(&mut self) -> bool {
            self.asked += 1;
            self.strict
        }
    }

    const CORNERS: [[i64; 3]; 2] = [[0; 3], [1 << 32; 3]];

    #[test]
    fn every_edit_needs_can_edit_world_before_any_zone_is_asked() {
        let mut zones = Answers::default();
        assert_eq!(
            refusal(BITS.edit_base, BITS, CORNERS, &mut zones),
            BuildRights::NO_CAN_EDIT_WORLD
        );
        assert_eq!(zones.asked, 0);
    }

    #[test]
    fn a_building_zone_needs_can_edit_base_and_elsewhere_it_is_not_asked() {
        let mut inside = Answers {
            touches: true,
            ..Answers::default()
        };
        assert_eq!(
            refusal(BITS.edit_world, BITS, CORNERS, &mut inside),
            BuildRights::NO_CAN_EDIT_BASE
        );
        assert_eq!(
            refusal(BOTH, BITS, CORNERS, &mut inside),
            BuildRights::ALLOWED
        );
        let mut outside = Answers::default();
        assert_eq!(
            refusal(BITS.edit_world, BITS, CORNERS, &mut outside),
            BuildRights::ALLOWED
        );
    }

    #[test]
    fn either_corner_outside_the_building_area_refuses() {
        for corner in CORNERS {
            let mut zones = Answers {
                outside: Some(corner),
                ..Answers::default()
            };
            assert_eq!(
                refusal(BOTH, BITS, CORNERS, &mut zones),
                BuildRights::OUTSIDE_BUILDING_ZONE
            );
        }
    }

    #[test]
    fn a_strict_no_build_zone_refuses_even_with_every_permission() {
        let mut zones = Answers {
            strict: true,
            ..Answers::default()
        };
        assert_eq!(
            refusal(u8::MAX, BITS, CORNERS, &mut zones),
            BuildRights::INSIDE_STRICT_BUILD_INHIBITOR_ZONE
        );
    }

    #[test]
    fn permission_bits_come_from_their_indices() {
        assert_eq!(Permissions::from_indices(3, 2), Some(BITS));
        assert_eq!(Permissions::from_indices(8, 2), None);
        assert_eq!(Permissions::from_indices(u64::MAX, 2), None);
    }

    fn check(min: [f64; 3], max: [f64; 3]) -> BuildRights {
        BuildRights {
            header: Header::of::<BuildRights>(),
            player: 2,
            refusal: 0,
            min,
            max,
        }
    }

    #[test]
    fn a_box_that_is_not_finite_or_inside_out_is_refused() {
        assert_eq!(
            EditBox::new(&check([0.0, f64::NAN, 0.0], [1.0; 3])),
            Err(Status::BAD_ARGS)
        );
        assert_eq!(
            EditBox::new(&check([0.0; 3], [1.0, f64::INFINITY, 1.0])),
            Err(Status::BAD_ARGS)
        );
        assert_eq!(
            EditBox::new(&check([2.0, 0.0, 0.0], [1.0; 3])),
            Err(Status::BAD_ARGS)
        );
    }

    #[test]
    fn a_box_reaches_the_game_as_padded_metres_and_fixed_corners() {
        let edit = EditBox::new(&check([1.5, 2.0, -3.0], [4.0, 5.0, 6.0])).unwrap();
        assert_eq!(edit.to_game(), [1.5, 2.0, -3.0, 0.0, 4.0, 5.0, 6.0, 0.0]);
        assert_eq!(
            edit.corners(),
            [[3 << 31, 2 << 32, -(3 << 32)], [4 << 32, 5 << 32, 6 << 32]]
        );
    }
}

//! The fake's altars: their list, zones, limits and the base limit.

use enshwick_core::Entity;
use enshwick_core::abi::{AltarInfo, AltarLimit, AltarZone, Bounds, BuildingZoneCheck, Header};

use crate::test::{BaseCode, FakeHost, Installed, table};

impl FakeHost {
    /// Plays a game build whose base code the platform does not know:
    /// `raise_base_limit` and `request_base_limits` answer
    /// [`Status::MISSING`](enshwick_core::abi::Status::MISSING), and the game
    /// keeps its own limits.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().without_base_limit().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_base_limit(mut self) -> Self {
        self.base_code = BaseCode::Unknown;
        self
    }

    /// Plays a game build whose zone code the platform knows but not all of
    /// the code bases past 16 need: requests up to the most go through, and
    /// once the mod is armed the game holds at most 16 bases, which
    /// `altars::base_limit` reads as `bases` and `max_bases`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().without_bases_past_16().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_bases_past_16(mut self) -> Self {
        self.base_code = BaseCode::ZonesOnly;
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Makes `altars` the altars of the world, as the loader lists them to a
    /// mod that opened altars, and raises
    /// [`AltarsChanged`](enshwick_core::events::AltarsChanged) as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::AltarInfo;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let mut altar = AltarInfo::EMPTY;
    /// altar.entity = enshwick::Entity(4096);
    /// loader.set_altars(&[altar]);
    /// ```
    pub fn set_altars(&self, altars: &[AltarInfo]) {
        table::with((), |s| s.altars = altars.to_vec());
        let _handled = self.raise::<enshwick_core::events::AltarsChanged>(&());
    }

    /// Whether the mod opened altars, and whether it controls their zones.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.altars_opened(), (false, false));
    /// ```
    #[must_use]
    pub fn altars_opened(&self) -> (bool, bool) {
        table::with((false, false), |s| (s.altars_opened, s.zone_control))
    }

    /// Runs the mod's altar zone filters on one altar's vanilla zone of
    /// `height` cells, as the loader would, a `grow` below 0 counting as 0 and
    /// a `height` below one cell keeping the one before; the `grow` and
    /// `height` they leave.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.altar_zone(enshwick::Entity(4096), 2, 60), (0, 60));
    /// ```
    #[must_use]
    pub fn altar_zone(&self, altar: Entity, level: u8, height: i64) -> (i64, i64) {
        let filters = table::with(Vec::new(), |s| s.zone_sizing.filters.clone());
        let mut zone = AltarZone {
            header: Header::of::<AltarZone>(),
            altar: Entity(altar.0),
            level,
            reserved: [0; 3],
            grow: 0,
            height,
        };
        for filter in filters {
            let before = zone;
            // SAFETY: a valid registration; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe { callback(&raw mut zone, user) });
            zone.grow = zone.grow.max(0);
            if zone.height <= 0 {
                zone.height = before.height;
            }
        }
        (zone.grow, zone.height)
    }

    /// How often the mod asked for the altar zones to be sized again.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.zone_resizes(), 0);
    /// ```
    #[must_use]
    pub fn zone_resizes(&self) -> usize {
        table::with(0, |s| s.zone_sizing.resizes)
    }

    /// Asks the mod's zone check filters about a placement the game refused,
    /// as the loader would; whether any accepts it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Bounds;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.building_zone_check(&[], Bounds::default(), 0));
    /// ```
    #[must_use]
    pub fn building_zone_check(&self, zones: &[Bounds], object: Bounds, margin: i64) -> bool {
        let filters = table::with(Vec::new(), |s| s.zone_filters.clone());
        let call = BuildingZoneCheck {
            header: Header::of::<BuildingZoneCheck>(),
            zones: zones.as_ptr(),
            zone_count: zones.len(),
            object,
            margin,
        };
        filters.iter().any(|filter| {
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            filter.run(|callback, user| unsafe { callback(&raw const call, user) != 0 })
        })
    }

    /// Asks the mod's altar limit filters about `flame_level`, where the game
    /// allows `vanilla`, as the loader would: each on the count the last one
    /// left, the answer kept within the zones the game groups.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.altar_limit(3, 6), 6);
    /// ```
    #[must_use]
    pub fn altar_limit(&self, flame_level: u32, vanilla: u8) -> u8 {
        let filters = table::with(Vec::new(), |s| s.altar_limits.clone());
        let mut limit = AltarLimit {
            header: Header::of::<AltarLimit>(),
            flame_level,
            vanilla,
            allowed: vanilla,
            reserved: [0; 2],
        };
        for filter in filters {
            // SAFETY: a valid limit; `user` is the mod's pointer.
            filter.run(|callback, user| unsafe { callback(&raw mut limit, user) });
            limit.flame_level = flame_level;
            limit.vanilla = vanilla;
        }
        let (zones, _) = table::world::altars::zones_and_bases();
        limit.allowed.min(u8::try_from(zones).unwrap_or(u8::MAX))
    }

    /// The largest altar zones and bases the mod asked for, each at least
    /// the game's own; `None` until it asks.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.requested_base_limits(), None);
    /// ```
    #[must_use]
    pub fn requested_base_limits(&self) -> Option<(u32, u32)> {
        table::with(None, |s| s.base_limit.requested)
    }

    /// Makes the last grouping find `zones` altar zones in `bases` bases.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_grouping(17, 16);
    /// ```
    pub fn set_grouping(&self, zones: u32, bases: u32) {
        table::with((), |s| s.grouping = Some((zones, bases)));
    }
}

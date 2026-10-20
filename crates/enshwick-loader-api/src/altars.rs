//! Flame Altars of the server's world: where they stand, their levels, their
//! building zones and the bases those zones form; the sizes a mod gives the
//! zones; and building across the seams between zones.
//!
//! The loader follows the altars in the game server: the dedicated server, or
//! a client while it hosts a game. Zones are in whole 0.5 m cells, half-open
//! (`min <= p < max`) like the engine's point test, with axis 1 up; the
//! footprint is axes 0 and 2.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::events::AltarsChanged;
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let altars = load.altars()?;
//!     load.on::<AltarsChanged>(move |()| {
//!         let list = altars.list();
//!         let bases = enshwick_loader_api::altars::bases_of(&list);
//!         enshwick_loader_api::info!("{} altars in {} bases", list.len(), bases.len());
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{AltarInfo, AltarZoneSize, Bounds, HostApi, ZoneCheck as RawZoneCheck};
use crate::ecs::Entity;
use crate::{Host, Load, Off};

/// A world-aligned, half-open box of whole 0.5 m cells; axis 1 is up.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::CellBox;
///
/// let zone = CellBox::new([0, 0, 0], [80, 80, 80]);
/// assert!(zone.contains([79, 0, 0]));
/// assert!(!zone.contains([80, 0, 0]));
/// assert_eq!(zone.footprint_area(), 6400);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellBox {
    /// The inclusive lower corner.
    pub min: [i64; 3],
    /// The exclusive upper corner.
    pub max: [i64; 3],
}

impl CellBox {
    /// The box from `min` (inclusive) to `max` (exclusive).
    ///
    /// # Examples
    ///
    /// See [`CellBox`].
    #[must_use]
    pub const fn new(min: [i64; 3], max: [i64; 3]) -> Self {
        Self { min, max }
    }

    /// Whether the box holds no cell.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::altars::CellBox;
    ///
    /// assert!(CellBox::default().is_empty());
    /// assert!(!CellBox::new([0, 0, 0], [1, 1, 1]).is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        (0..3).any(|a| self.min[a] >= self.max[a])
    }

    /// Whether the cell `cell` lies inside.
    ///
    /// # Examples
    ///
    /// See [`CellBox`].
    #[must_use]
    pub fn contains(&self, cell: [i64; 3]) -> bool {
        (0..3).all(|a| self.min[a] <= cell[a] && cell[a] < self.max[a])
    }

    /// Whether the two boxes share a cell boundary or more, edges and corners
    /// included, as zones that the engine groups into one base do.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::altars::CellBox;
    ///
    /// let a = CellBox::new([0, 0, 0], [10, 10, 10]);
    /// assert!(a.touches(&CellBox::new([10, 0, 0], [20, 10, 10])));
    /// assert!(!a.touches(&CellBox::new([11, 0, 0], [20, 10, 10])));
    /// ```
    #[must_use]
    pub fn touches(&self, other: &Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && (0..3).all(|a| self.min[a] <= other.max[a] && other.min[a] <= self.max[a])
    }

    /// The smallest box holding both.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::altars::CellBox;
    ///
    /// let a = CellBox::new([0, 0, 0], [10, 10, 10]);
    /// let b = CellBox::new([10, -5, 0], [20, 10, 10]);
    /// assert_eq!(a.union(&b), CellBox::new([0, -5, 0], [20, 10, 10]));
    /// ```
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: [0, 1, 2].map(|a| self.min[a].min(other.min[a])),
            max: [0, 1, 2].map(|a| self.max[a].max(other.max[a])),
        }
    }

    /// The footprint's area in cells: axes 0 and 2.
    ///
    /// # Examples
    ///
    /// See [`CellBox`].
    #[must_use]
    pub fn footprint_area(&self) -> i64 {
        if self.is_empty() {
            return 0;
        }
        (self.max[0] - self.min[0]) * (self.max[2] - self.min[2])
    }

    const fn from_bounds(bounds: Bounds) -> Self {
        Self {
            min: bounds.min,
            max: bounds.max,
        }
    }
}

/// The cell that holds a point given in the engine's 32.32 fixed-point
/// metres, as `CurrentTransform` stores positions.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::cell_of_fixed;
///
/// assert_eq!(cell_of_fixed([3 << 32, 1 << 31, -1]), [6, 1, -1]);
/// ```
#[must_use]
pub const fn cell_of_fixed(fixed: [i64; 3]) -> [i64; 3] {
    [fixed[0] >> 31, fixed[1] >> 31, fixed[2] >> 31]
}

/// The cell that holds a point given in metres.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::cell_of_metres;
///
/// assert_eq!(cell_of_metres([3.0, 0.75, -0.25]), [6, 1, -1]);
/// ```
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates fit in i64 cells"
)]
pub fn cell_of_metres(metres: [f64; 3]) -> [i64; 3] {
    metres.map(|m| (m * 2.0).floor() as i64)
}

/// One Flame Altar.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::{Altar, CellBox};
/// use enshwick_loader_api::ecs::Entity;
///
/// let altar = Altar {
///     entity: Entity(4096),
///     level: 1,
///     tilted: false,
///     position: [0.0; 3],
///     zone: CellBox::new([-40, -40, -40], [40, 40, 40]),
///     levels: vec![CellBox::new([-40, -40, -40], [40, 40, 40])],
/// };
/// assert!(altar.zone.contains([0, 0, 0]));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Altar {
    /// The altar's entity.
    pub entity: Entity,
    /// Its Flame level; 0 until the engine has reported it.
    pub level: u8,
    /// Tilted off the vertical: its zone is unknown and empty, and it keeps
    /// the engine's size.
    pub tilted: bool,
    /// Where it stands, in metres.
    pub position: [f64; 3],
    /// Its zone as the engine has built it now, growth by any mod included.
    pub zone: CellBox,
    /// Its vanilla zone at each level from 1 to the current one, each inside
    /// the next; empty while the level is unknown.
    pub levels: Vec<CellBox>,
}

impl Altar {
    fn from_info(info: &AltarInfo) -> Self {
        let count = usize::from(info.level_count).min(info.levels.len());
        Self {
            entity: Entity(info.entity),
            level: info.level,
            tilted: info.tilted != 0,
            position: info.position,
            zone: CellBox::from_bounds(info.zone),
            levels: info.levels[..count]
                .iter()
                .copied()
                .map(CellBox::from_bounds)
                .collect(),
        }
    }
}

/// Altars whose zones touch or overlap, which the engine groups into one
/// base.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::{bases_of, Altar, CellBox};
/// use enshwick_loader_api::ecs::Entity;
///
/// let altar = |entity, x| Altar {
///     entity: Entity(entity),
///     level: 1,
///     tilted: false,
///     position: [0.0; 3],
///     zone: CellBox::new([x, 0, 0], [x + 10, 10, 10]),
///     levels: Vec::new(),
/// };
/// let bases = bases_of(&[altar(1, 0), altar(2, 10), altar(3, 50)]);
/// assert_eq!(bases.len(), 2);
/// assert_eq!(bases[0].altars, vec![Entity(1), Entity(2)]);
/// assert!(bases[0].contains([15, 5, 5]));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Base {
    /// Its altars, by entity, smallest first.
    pub altars: Vec<Entity>,
    /// Their zones, in the same order.
    pub zones: Vec<CellBox>,
    /// The smallest box holding every zone.
    pub bounds: CellBox,
}

impl Base {
    /// Whether one of the base's zones holds the cell `cell`.
    ///
    /// # Examples
    ///
    /// See [`Base`].
    #[must_use]
    pub fn contains(&self, cell: [i64; 3]) -> bool {
        self.zones.iter().any(|zone| zone.contains(cell))
    }
}

/// Groups `altars` into bases: altars whose zones touch or overlap, directly
/// or through others, form one. Altars with an empty zone form none. Bases
/// come in the order of their smallest altar entity.
///
/// # Examples
///
/// See [`Base`].
#[must_use]
pub fn bases_of(altars: &[Altar]) -> Vec<Base> {
    let mut placed: Vec<&Altar> = altars.iter().filter(|a| !a.zone.is_empty()).collect();
    placed.sort_by_key(|a| a.entity.0);
    let mut group: Vec<usize> = (0..placed.len()).collect();
    for i in 0..placed.len() {
        for j in i + 1..placed.len() {
            if placed[i].zone.touches(&placed[j].zone) {
                let (a, b) = (root(&mut group, i), root(&mut group, j));
                group[a.max(b)] = a.min(b);
            }
        }
    }
    let mut bases: Vec<(usize, Base)> = Vec::new();
    for (i, altar) in placed.iter().enumerate() {
        let r = root(&mut group, i);
        let at = if let Some(at) = bases.iter().position(|(key, _)| *key == r) {
            at
        } else {
            bases.push((
                r,
                Base {
                    bounds: altar.zone,
                    ..Base::default()
                },
            ));
            bases.len() - 1
        };
        let base = &mut bases[at].1;
        base.altars.push(altar.entity);
        base.zones.push(altar.zone);
        base.bounds = base.bounds.union(&altar.zone);
    }
    bases.into_iter().map(|(_, base)| base).collect()
}

/// The root of `i`'s group, halving the path on the way.
fn root(group: &mut [usize], mut i: usize) -> usize {
    while group[i] != i {
        group[i] = group[group[i]];
        i = group[i];
    }
    i
}

/// The altars, from [`Load::altars`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Altars {
    host: Host,
}

impl Altars {
    /// Every altar of the server's world as it is now; empty while no world
    /// runs or when the loader refuses. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn levels(altars: &enshwick_loader_api::altars::Altars) -> Vec<u8> {
    ///     altars.list().iter().map(|altar| altar.level).collect()
    /// }
    /// ```
    #[must_use]
    pub fn list(&self) -> Vec<Altar> {
        let api = self.host.api();
        let entry_size = u32::try_from(size_of::<AltarInfo>()).unwrap_or(u32::MAX);
        let mut count = 0u32;
        let status = (api.altars)(
            self.host.handle(),
            core::ptr::null_mut(),
            entry_size,
            0,
            &raw mut count,
        );
        if !status.is_ok() {
            return Vec::new();
        }
        // A few more, in case altars were added between the calls.
        let mut infos = vec![AltarInfo::EMPTY; count as usize + 8];
        let capacity = u32::try_from(infos.len()).unwrap_or(u32::MAX);
        let status = (api.altars)(
            self.host.handle(),
            infos.as_mut_ptr(),
            entry_size,
            capacity,
            &raw mut count,
        );
        if !status.is_ok() {
            return Vec::new();
        }
        infos.truncate((count as usize).min(infos.len()));
        infos.iter().map(Altar::from_info).collect()
    }

    /// The bases the altars form now; see [`bases_of`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn based(altars: &enshwick_loader_api::altars::Altars, cell: [i64; 3]) -> bool {
    ///     altars.bases().iter().any(|base| base.contains(cell))
    /// }
    /// ```
    #[must_use]
    pub fn bases(&self) -> Vec<Base> {
        bases_of(&self.list())
    }
}

/// The size a mod gives one altar's zone; see [`ZoneControl::set`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::ZoneSize;
/// use enshwick_loader_api::ecs::Entity;
///
/// let size = ZoneSize { altar: Entity(4096), level: 2, grow: 6, height: 120 };
/// assert_eq!(size.grow, 6);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneSize {
    /// The altar.
    pub altar: Entity,
    /// The level the size is for: the loader applies it while the altar is
    /// at this level, the vanilla size otherwise.
    pub level: u8,
    /// Cells added to the vanilla zone of that level on every horizontal
    /// side, at least 0.
    pub grow: i64,
    /// The zone's height in cells.
    pub height: i64,
}

/// The right to size altar zones, from [`Load::control_altar_zones`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::altars::ZoneSize;
/// use enshwick_loader_api::events::AltarsChanged;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let control = load.control_altar_zones()?;
///     load.on::<AltarsChanged>(move |()| {
///         // Every altar two cells wider on each side.
///         let sizes: Vec<ZoneSize> = control
///             .altars()
///             .list()
///             .iter()
///             .filter_map(|altar| {
///                 let zone = altar.levels.last()?;
///                 Some(ZoneSize {
///                     altar: altar.entity,
///                     level: altar.level,
///                     grow: 2,
///                     height: zone.max[1] - zone.min[1],
///                 })
///             })
///             .collect();
///         let _ = control.set(&sizes);
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ZoneControl {
    host: Host,
}

impl ZoneControl {
    /// The altars.
    ///
    /// # Examples
    ///
    /// See [`ZoneControl`].
    #[must_use]
    pub const fn altars(&self) -> Altars {
        Altars { host: self.host }
    }

    /// Replaces the sizes given before: each altar in `sizes` gets its size
    /// on the server's next zone pass, every other altar its vanilla size.
    /// Any thread. A size for an altar the loader does not know, or for
    /// another level than the altar's, is left out.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the call.
    ///
    /// # Examples
    ///
    /// See [`ZoneControl`].
    pub fn set(&self, sizes: &[ZoneSize]) -> Result<(), Off> {
        let raw: Vec<AltarZoneSize> = sizes
            .iter()
            .map(|size| AltarZoneSize {
                altar: size.altar.0,
                level: size.level,
                reserved: [0; 3],
                grow: size.grow,
                height: size.height,
            })
            .collect();
        let status = (self.host.api().set_altar_zones)(self.host.handle(), raw.as_ptr(), raw.len());
        crate::load::ok(status, (), || "altar zone sizes".to_owned())
    }
}

/// The most zones a [`ZoneCheck`] carries; the engine keeps at most 255.
pub const MAX_ZONES: usize = 256;

/// A placement the game refused because the object does not lie in a single
/// building zone; see [`Load::on_zone_check`]. Coordinates are 32.32
/// fixed-point metres.
///
/// # Examples
///
/// See [`Load::on_zone_check`].
#[derive(Clone, Copy, Debug)]
pub struct ZoneCheck<'a> {
    /// The building zones, half-open.
    pub zones: &'a [Bounds],
    /// The object's box, both corners included.
    pub object: Bounds,
    /// How far the game lets an object reach past a zone, on every side.
    pub margin: i64,
}

unsafe extern "system" fn run_zone_check<F>(check: *const RawZoneCheck, user: *mut c_void) -> u8
where
    F: Fn(&ZoneCheck<'_>) -> bool,
{
    // SAFETY: the loader passes a valid check for the duration of the call.
    let Some(check) = (unsafe { check.as_ref() }) else {
        return 0;
    };
    let zones = if check.zones.is_null() || check.zone_count == 0 {
        &[][..]
    } else {
        // SAFETY: `zone_count` boxes at `zones`, valid for the call.
        unsafe { core::slice::from_raw_parts(check.zones, check.zone_count) }
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let call = ZoneCheck {
        zones,
        object: check.object,
        margin: check.margin,
    };
    u8::from(crate::host::guard(
        "panic in a zone check",
        || false,
        || filter(&call),
    ))
}

impl Load<'_> {
    /// Asks the loader to follow the Flame Altars, for [`Altars::list`] and
    /// [`AltarsChanged`](crate::events::AltarsChanged).
    ///
    /// # Errors
    ///
    /// [`Off`] after init, with a loader older than the API, or on a game
    /// build that lacks the altar helpers.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn altars(&mut self) -> Result<Altars, Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, altars_open),
            "altars",
        )?;
        let status = (host.api().altars_open)(host.handle());
        crate::load::ok(status, Altars { host }, || "altars".to_owned())
    }

    /// Makes the mod the one that sizes altar zones; it follows the altars
    /// too. One mod at a time.
    ///
    /// # Errors
    ///
    /// [`Off`] when another mod sizes them, and as for [`Load::altars`].
    ///
    /// # Examples
    ///
    /// See [`ZoneControl`].
    pub fn control_altar_zones(&mut self) -> Result<ZoneControl, Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, altar_zones_control),
            "altar zones",
        )?;
        let status = (host.api().altar_zones_control)(host.handle());
        crate::load::ok(status, ZoneControl { host }, || {
            "altar zones, sized by one mod at a time".to_owned()
        })
    }

    /// Runs `filter` when the game refuses a placement because the object's
    /// box does not lie in a single building zone; returning `true` accepts
    /// it after all. It runs on both sides, on the thread that checks the
    /// placement, every time a player aims a piece: keep it cheap and never
    /// block.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     // Accept an object whose box lies in two zones' bounding box.
    ///     load.on_zone_check(|check| {
    ///         check.zones.len() >= 2
    ///             && (0..3).all(|a| {
    ///                 let low = check.zones.iter().map(|z| z.min[a]).min().unwrap_or(0);
    ///                 let high = check.zones.iter().map(|z| z.max[a]).max().unwrap_or(0);
    ///                 low <= check.object.min[a] && check.object.max[a] < high
    ///             })
    ///     })
    /// }
    /// ```
    pub fn on_zone_check<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&ZoneCheck<'_>) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, zone_check_filter),
            "zone checks",
        )?;
        crate::load::hand_over(
            filter,
            |user| (host.api().zone_check_filter)(host.handle(), run_zone_check::<F>, user),
            "zone checks",
        )
    }
}

/// How many altar zones and bases the game's last grouping found, from
/// [`grouping`].
///
/// # Examples
///
/// See [`grouping`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grouping {
    /// Altar zones in the world.
    pub zones: u32,
    /// Bases they form.
    pub bases: u32,
}

/// The game's last grouping of altar zones into bases, while the raised base
/// limit ([`Load::raise_base_limit`]) is in force; `None` before the first
/// grouping. Any thread.
///
/// # Examples
///
/// ```
/// let past_vanilla = enshwick_loader_api::altars::grouping().is_some_and(|g| g.zones > 16);
/// # let _ = past_vanilla;
/// ```
#[must_use]
pub fn grouping() -> Option<Grouping> {
    let host = crate::host()?;
    let end = core::mem::offset_of!(HostApi, base_zones) + size_of::<usize>();
    if !host.api().header.covers(end) {
        return None;
    }
    let (mut zones, mut bases) = (0u32, 0u32);
    let status = (host.api().base_zones)(host.handle(), &raw mut zones, &raw mut bases);
    status.is_ok().then_some(Grouping { zones, bases })
}

unsafe extern "system" fn run_limit<F>(flame_level: u32, user: *mut c_void) -> u8
where
    F: Fn(u32) -> u8,
{
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let limit = unsafe { &*user.cast::<F>() };
    crate::host::guard("panic in an altar limit", || 16, || limit(flame_level))
}

impl Load<'_> {
    /// Makes `limit` decide how many altars each Flame level allows, on both
    /// sides; one mod at a time. Values past 16 need
    /// [`Load::raise_base_limit`] to group.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, when another mod decides, on a build that lacks
    /// the lookup, or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.on_altar_limit(|level| u8::try_from(2 + 2 * level).unwrap_or(16).min(16))
    /// }
    /// ```
    pub fn on_altar_limit<F>(&mut self, limit: F) -> Result<(), Off>
    where
        F: Fn(u32) -> u8 + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, altar_limit),
            "the altar limit",
        )?;
        crate::load::hand_over(
            limit,
            |user| (host.api().altar_limit)(host.handle(), run_limit::<F>, user),
            "the altar limit, set by one mod at a time",
        )
    }

    /// Lets the game group 32 altar zones into its 16 bases, refusing an
    /// altar that would start a 17th base; `Ok(false)` when the platform does
    /// not recognise this build's code, and the game keeps 16. The patches
    /// count as the mod's own: one the game refuses at arming turns the mod
    /// off.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let cap = if load.raise_base_limit()? { 32 } else { 16 };
    ///     enshwick_loader_api::info!("up to {cap} altar zones");
    ///     Ok(())
    /// }
    /// ```
    pub fn raise_base_limit(&mut self) -> Result<bool, Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, raise_base_limit),
            "the base limit",
        )?;
        let status = (host.api().raise_base_limit)(host.handle());
        if status.is_ok() {
            Ok(true)
        } else if status == crate::abi::Status::MISSING {
            Ok(false)
        } else {
            Err(Off::status("the base limit", status))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn altar(entity: u32, min: [i64; 3], max: [i64; 3]) -> Altar {
        Altar {
            entity: Entity(entity),
            level: 1,
            tilted: false,
            position: [0.0; 3],
            zone: CellBox::new(min, max),
            levels: Vec::new(),
        }
    }

    #[test]
    fn touching_zones_form_one_base_through_a_chain() {
        let bases = bases_of(&[
            altar(3, [20, 0, 0], [30, 10, 10]),
            altar(1, [0, 0, 0], [10, 10, 10]),
            altar(2, [10, 0, 0], [20, 10, 10]),
            altar(4, [100, 0, 0], [110, 10, 10]),
        ]);
        assert_eq!(bases.len(), 2);
        assert_eq!(bases[0].altars, vec![Entity(1), Entity(2), Entity(3)]);
        assert_eq!(bases[0].bounds, CellBox::new([0, 0, 0], [30, 10, 10]));
        assert_eq!(bases[1].altars, vec![Entity(4)]);
    }

    #[test]
    fn an_altar_without_a_zone_forms_no_base() {
        let mut tilted = altar(1, [0; 3], [0; 3]);
        tilted.tilted = true;
        assert!(bases_of(&[tilted]).is_empty());
    }

    #[test]
    fn cells_round_down() {
        assert_eq!(cell_of_metres([-0.1, 0.0, 0.49]), [-1, 0, 0]);
        assert_eq!(cell_of_fixed([-1, 0, (1 << 31) - 1]), [-1, 0, 0]);
    }
}

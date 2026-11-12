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
//! use enshwick::events::AltarsChanged;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let altars = load.altars()?;
//!     load.on::<AltarsChanged>(move |()| {
//!         let list = altars.list();
//!         let bases = enshwick::altars::bases_of(&list);
//!         enshwick::info!("{} altars in {} bases", list.len(), bases.len());
//!     })
//! }
//! ```

mod filters;
pub(crate) mod kinds;
pub(crate) mod limits;
mod script;

pub use kinds::{KindZone, NewFlameAltar};

use filters::{run_building_zone_check, run_limit, run_zone};

use crate::Entity;
use crate::abi::{AltarInfo, Bounds};
use crate::content::Guid;
use crate::{Host, Off};

/// A world-aligned, half-open box of whole 0.5 m cells; axis 1 is up.
///
/// # Examples
///
/// ```
/// use enshwick::altars::CellBox;
///
/// let zone = CellBox::new([0, 0, 0], [80, 80, 80]);
/// assert!(zone.contains([79, 0, 0]));
/// assert!(!zone.contains([80, 0, 0]));
/// assert_eq!(zone.footprint_area(), 6400);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
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
    /// use enshwick::altars::CellBox;
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
    /// use enshwick::altars::CellBox;
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
    /// use enshwick::altars::CellBox;
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
/// use enshwick::altars::cell_of_fixed;
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
/// use enshwick::altars::cell_of_metres;
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
/// use enshwick::altars::{Altar, CellBox};
/// use enshwick::Entity;
///
/// let altar = Altar {
///     entity: Entity(4096),
///     level: 1,
///     tilted: false,
///     position: [0.0; 3],
///     zone: CellBox::new([-40, -40, -40], [40, 40, 40]),
///     levels: vec![CellBox::new([-40, -40, -40], [40, 40, 40])],
///     template: None,
/// };
/// assert!(altar.zone.contains([0, 0, 0]));
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
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
    /// Its template, which names its kind: the vanilla altar's or one from
    /// [`Load::add_flame_altar`](../struct.Load.html#method.add_flame_altar); `None` off the server's game thread.
    pub template: Option<Guid>,
}

impl Altar {
    fn from_info(host: Host, info: &AltarInfo) -> Self {
        let count = usize::from(info.level_count).min(info.levels.len());
        Self {
            template: template_of(host, info.entity),
            entity: info.entity,
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
/// use enshwick::altars::{bases_of, Altar, CellBox};
/// use enshwick::Entity;
///
/// let altar = |entity, x| Altar {
///     entity: Entity(entity),
///     level: 1,
///     tilted: false,
///     position: [0.0; 3],
///     zone: CellBox::new([x, 0, 0], [x + 10, 10, 10]),
///     levels: Vec::new(),
///     template: None,
/// };
/// let bases = bases_of(&[altar(1, 0), altar(2, 10), altar(3, 50)]);
/// assert_eq!(bases.len(), 2);
/// assert_eq!(bases[0].altars, vec![Entity(1), Entity(2)]);
/// assert!(bases[0].contains([15, 5, 5]));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
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

/// The altars, from [`Load::altars`](../struct.Load.html#method.altars).
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Altars {
    host: Host,
}

#[crate::script(list)]
impl Altars {
    /// Every altar of the server's world as it is now; empty while no world
    /// runs or when the loader refuses. Any thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn levels(altars: &enshwick::altars::Altars) -> Vec<u8> {
    ///     altars.list().iter().map(|altar| altar.level).collect()
    /// }
    /// ```
    #[must_use]
    #[script]
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
        infos
            .iter()
            .map(|info| Altar::from_info(self.host, info))
            .collect()
    }

    /// The bases the altars form now; see [`bases_of`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn based(altars: &enshwick::altars::Altars, cell: [i64; 3]) -> bool {
    ///     altars.bases().iter().any(|base| base.contains(cell))
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn bases(&self) -> Vec<Base> {
        bases_of(&self.list())
    }
}

/// One altar's zone as the loader sizes it; see [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone).
/// A filter sees the size the filters before it left, the vanilla one first,
/// and may change `grow` and `height`.
///
/// # Examples
///
/// See [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[script(context)]
#[non_exhaustive]
pub struct AltarZone {
    /// The altar.
    pub altar: Entity,
    /// The altar's level.
    pub level: u8,
    /// Cells added to the vanilla zone on every horizontal side, at least 0;
    /// 0 keeps the vanilla width.
    pub grow: i64,
    /// The zone's height in cells.
    pub height: i64,
}

/// The mod's place among the altar zone filters, from
/// [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone): the altars, and a way to have the zones sized
/// again.
///
/// # Examples
///
/// See [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone).
#[derive(Clone, Copy, Debug)]
pub struct ZoneSizes {
    host: Host,
}

#[crate::script(sizes)]
impl ZoneSizes {
    /// The altars.
    ///
    /// # Examples
    ///
    /// See [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone).
    #[must_use]
    #[script]
    pub const fn altars(&self) -> Altars {
        Altars { host: self.host }
    }

    /// Has the loader size the altar zones again on the server's next step,
    /// as when the mod's settings change what its filter answers. Any
    /// thread.
    ///
    /// # Errors
    ///
    /// [`ResizeError`] when the loader refuses the call.
    ///
    /// # Examples
    ///
    /// See [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone).
    #[script]
    pub fn changed(&self) -> Result<(), ResizeError> {
        match (self.host.api().altar_zones_changed)(self.host.handle()) {
            crate::abi::Status::OK => Ok(()),
            crate::abi::Status::FAULTED => Err(ResizeError::TurnedOff),
            other => Err(ResizeError::Refused(other.0)),
        }
    }
}

/// Why the loader did not size the altar zones again for
/// [`ZoneSizes::changed`].
///
/// # Examples
///
/// ```
/// use enshwick::altars::ResizeError;
///
/// assert_eq!(
///     ResizeError::Refused(3).to_string(),
///     "altar zones not sized again: refused by the loader (status 3)"
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResizeError {
    /// The mod is turned off, so its filter no longer runs.
    #[error("altar zones not sized again: the mod is turned off")]
    TurnedOff,
    /// The loader refused for another reason, with its status.
    #[error("altar zones not sized again: refused by the loader (status {0})")]
    Refused(u32),
}

crate::load::off_from!(ResizeError);

/// The most zones a [`BuildingZoneCheck`] carries; the engine keeps at most 255.
pub const MAX_ZONES: usize = 256;

/// How many altars a Flame level allows, as an altar limit filter sees it;
/// see [`Load::on_altar_limit`](../struct.Load.html#method.on_altar_limit).
///
/// # Examples
///
/// See [`Load::on_altar_limit`](../struct.Load.html#method.on_altar_limit).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[script(context)]
#[non_exhaustive]
pub struct AltarLimit {
    /// The Flame level.
    pub flame_level: u32,
    /// The altars the game itself allows at that level.
    pub vanilla: u8,
    /// The altars allowed so far, as the filters before have left it; the
    /// filter's answer.
    pub allowed: u8,
}

/// The most altar zones and bases the game groups now; see [`base_limit`].
///
/// # Examples
///
/// See [`base_limit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct BaseLimit {
    /// Altar zones the game puts into bases.
    pub zones: u32,
    /// Bases the game holds.
    pub bases: u32,
    /// Altar zones the game's own grouping takes, without the raised limit.
    pub vanilla_zones: u32,
    /// Bases the game's own code holds.
    pub vanilla_bases: u32,
    /// The most altar zones a mod may ask for.
    pub max_zones: u32,
    /// The most bases a mod may ask for.
    pub max_bases: u32,
}

impl BaseLimit {
    /// Whether a world of `grouping` holds more altar zones or bases than
    /// the game's own code takes, so it must not open without the raised
    /// limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::{base_limit, grouping};
    ///
    /// let held = grouping().zip(base_limit()).is_some_and(|(g, l)| l.past_vanilla(g));
    /// # let _ = held;
    /// ```
    #[must_use]
    pub const fn past_vanilla(&self, grouping: Grouping) -> bool {
        grouping.zones > self.vanilla_zones || grouping.bases > self.vanilla_bases
    }
}

/// The most altar zones and bases the game groups now; `None` without a
/// loader. Any thread.
///
/// That is the game's own, or the largest request once either base limit
/// request ([`Load::raise_base_limit`](../struct.Load.html#method.raise_base_limit), `Load::request_base_limits`) is in
/// force, which is from [`Mod::ready`](../trait.Mod.html#method.ready) on, after the
/// loader arms its hooks; bases past 16 hold only where the loader armed
/// every hook they need, and [`BaseLimit::max_bases`] then reads 179.
///
/// # Examples
///
/// ```
/// use enshwick::altars::{base_limit, grouping};
///
/// let full = grouping().zip(base_limit()).is_some_and(|(g, l)| g.zones >= l.zones);
/// # let _ = full;
/// ```
#[must_use]
pub fn base_limit() -> Option<BaseLimit> {
    let host = crate::host()?;
    let mut out = crate::abi::BaseLimitInfo {
        header: crate::abi::Header::of::<crate::abi::BaseLimitInfo>(),
        zones: 0,
        bases: 0,
        vanilla_zones: 0,
        reserved: 0,
        vanilla_bases: 0,
        max_zones: 0,
        max_bases: 0,
    };
    let status = (host.api().base_limit)(host.handle(), &raw mut out);
    status.is_ok().then_some(BaseLimit {
        zones: out.zones,
        bases: out.bases,
        vanilla_zones: out.vanilla_zones,
        vanilla_bases: out.vanilla_bases,
        max_zones: out.max_zones,
        max_bases: out.max_bases,
    })
}

/// Caps the bases the server holds in this world at `bases`, below the most
/// the mod asked for at init.
///
/// An altar that would start a base past the cap is refused. Each call replaces the mod's own cap, the lowest cap of every
/// mod holds, and a cap at or past the request leaves the request; so a mod
/// asks for its most at init and sets each world's cap from its settings as
/// the world loads. [`base_limit`] reads the capped bases. Any thread.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Errors
///
/// [`Off`] below the game's 16 bases ([`BaseLimit::vanilla_bases`]) or
/// without a loader.
///
/// # Examples
///
/// ```
/// fn apply(bases: u32) -> Result<(), enshwick::Off> {
///     enshwick::altars::set_base_cap(bases)
/// }
/// # let _ = apply;
/// ```
#[cfg(feature = "unstable")]
pub fn set_base_cap(bases: u32) -> Result<(), Off> {
    let host = crate::host().ok_or_else(|| Off::new("the base cap: no loader"))?;
    let status = (host.api().set_base_cap)(host.handle(), bases);
    if status.is_ok() {
        Ok(())
    } else {
        Err(crate::load::off_status("the base cap", status))
    }
}

/// A placement the game refused because the object does not lie in a single
/// building zone; see [`Load::on_building_zone_check`](../struct.Load.html#method.on_building_zone_check). Coordinates are 32.32
/// fixed-point metres.
///
/// The name is enshwick's, after the game's building zone: the check the game makes has no name, and its refusal is the cursor
/// flag `CursorOutsideBuildingZone`.
///
/// # Examples
///
/// See [`Load::on_building_zone_check`](../struct.Load.html#method.on_building_zone_check).
#[derive(Clone, Copy, Debug)]
pub struct BuildingZoneCheck<'a> {
    /// The building zones, half-open.
    pub zones: &'a [Bounds],
    /// The object's box, both corners included.
    pub object: Bounds,
    /// How far the game lets an object reach past a zone, on every side.
    pub margin: i64,
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::altars`.
    #[doc(hidden)]
    fn altars(&mut self) -> Result<Altars, Off>;

    /// As `Load::on_altar_zone`.
    #[doc(hidden)]
    fn on_altar_zone<F>(&mut self, filter: F) -> Result<ZoneSizes, Off>
    where
        F: Fn(&mut AltarZone) + Send + Sync + 'static;

    /// As `Load::on_building_zone_check`.
    #[doc(hidden)]
    fn on_building_zone_check<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BuildingZoneCheck<'_>) -> bool + Send + Sync + 'static;

    /// As `Load::on_building_zone_check_script`.
    #[doc(hidden)]
    fn on_building_zone_check_script<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BuildingZoneCheck<'_>) -> Option<bool> + Send + Sync + 'static;

    /// As `Load::on_altar_limit`.
    #[doc(hidden)]
    fn on_altar_limit<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut AltarLimit) + Send + Sync + 'static;
}

impl crate::script::Bound for Altars {
    const NAME: &'static str = "altars";
    const DOC: &'static str = "Flame Altars: their list, their bases, zones and limits.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] =
        &[Self::SCRIPT_LIST, Self::SCRIPT_HELPERS];

    fn resolve(load: &mut impl crate::Loading) -> Result<Self, Off> {
        load.base_mut().altars()
    }
}

impl LoadMethods for crate::BaseLoad {
    fn altars(&mut self) -> Result<Altars, Off> {
        let host = self.host();
        let status = (host.api().altars_open)(host.handle());
        crate::load::ok(status, Altars { host }, || "altars".to_owned())
    }

    fn on_altar_zone<F>(&mut self, filter: F) -> Result<ZoneSizes, Off>
    where
        F: Fn(&mut AltarZone) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().altar_zone_filter)(host.handle(), run_zone::<F>, user),
            "the altar zone filter",
        )?;
        Ok(ZoneSizes { host })
    }

    fn on_building_zone_check<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BuildingZoneCheck<'_>) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| {
                (host.api().building_zone_check_filter)(
                    host.handle(),
                    run_building_zone_check::<F>,
                    user,
                )
            },
            "building zone checks",
        )
    }

    fn on_building_zone_check_script<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BuildingZoneCheck<'_>) -> Option<bool> + Send + Sync + 'static,
    {
        self.on_building_zone_check(move |check| filter(check).unwrap_or(false))
    }

    fn on_altar_limit<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut AltarLimit) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().altar_limit_filter)(host.handle(), run_limit::<F>, user),
            "the altar limit filter",
        )
    }
}

/// How many altar zones and bases the game's last grouping found, from
/// [`grouping`].
///
/// # Examples
///
/// See [`grouping`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct Grouping {
    /// Altar zones in the world.
    pub zones: u32,
    /// Bases they form.
    pub bases: u32,
}

/// The game's last grouping of altar zones into bases, once either base
/// limit request ([`Load::raise_base_limit`](../struct.Load.html#method.raise_base_limit), `Load::request_base_limits`)
/// is in force; `None` before the first grouping. Any thread.
///
/// # Examples
///
/// ```
/// use enshwick::altars::grouping;
///
/// let bases = grouping().map_or(0, |g| g.bases);
/// # let _ = bases;
/// ```
#[must_use]
pub fn grouping() -> Option<Grouping> {
    let host = crate::host()?;
    let (mut zones, mut bases) = (0u32, 0u32);
    let status = (host.api().base_zones)(host.handle(), &raw mut zones, &raw mut bases);
    status.is_ok().then_some(Grouping { zones, bases })
}

/// `entity`'s template; `None` off the server's game thread or for an entity
/// the loader cannot read.
pub(crate) fn template_of(host: Host, entity: Entity) -> Option<Guid> {
    let mut out = [0u8; 16];
    let status = (host.api().entity_template)(host.handle(), entity, &raw mut out);
    status.is_ok().then_some(Guid(out))
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
            template: None,
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

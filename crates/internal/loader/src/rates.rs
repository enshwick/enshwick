//! Game rates mods multiply: each mod's declared rates and factors, and the
//! factor in force by the API's one combination rule. The loader never
//! writes a factor into the game's settings, which the server saves back to
//! its configuration: it scales what the game's own functions answer.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{RateId, Status};
pub(crate) use enshwick::rates::scale_whole;
use enshwick::rates::{ExperienceGroup, Rate, combine, is_valid_factor};

use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned::{Symbol, comfort, experience, item, loot, settings, skill};

/// For each rate, the build table symbols of the game functions whose
/// answer the loader scales; the rate works where every one resolves.
pub(crate) const LINKS: &[(RateId, &[Symbol])] = &[
    (
        RateId::EXPERIENCE_COMBAT,
        &[experience::SOURCE_FACTOR, experience::KILL],
    ),
    (RateId::EXPERIENCE_MINING, &[experience::SOURCE_FACTOR]),
    (RateId::EXPERIENCE_EXPLORATION, &[experience::SOURCE_FACTOR]),
    (RateId::WORKSHOP_SPEED, &[settings::FACTORY_SPEED]),
    (RateId::RESOURCE_DROPS, &[settings::RESOURCE_DROPS]),
    (RateId::COMFORT, &[comfort::BASE_REGISTER]),
    (RateId::SKILL_POINTS, &[skill::POINTS_PER_LEVEL]),
    (RateId::LOOT, &[loot::STACK_ROLL, item::INFO]),
];

/// The build table symbols `rate` links; empty for a rate the loader does
/// not know.
pub(crate) fn links(rate: RateId) -> &'static [Symbol] {
    LINKS
        .iter()
        .find(|&&(id, _)| id == rate)
        .map_or(&[], |&(_, symbols)| symbols)
}

/// The rate each of the game's experience source values counts under, by
/// value; `None` for a source the game gives no factor. `value` answers the
/// engine value of each source's catalogue id.
pub(crate) fn experience_rates(value: impl Fn(&str) -> Option<u64>) -> [Option<RateId>; 256] {
    let mut rates = [None; 256];
    for &(source, id) in enshwick::__private::EXPERIENCE_SOURCES {
        if let Some(slot) = value(id)
            .and_then(|v| usize::try_from(v).ok())
            .and_then(|at| rates.get_mut(at))
        {
            *slot = ExperienceGroup::of(source).map(|group| Rate::Experience(group).id());
        }
    }
    rates
}

/// The slowest workshop speed a rate puts in force, so a workshop never
/// stops.
pub(crate) const MIN_WORKSHOP_SPEED: f32 = 0.01;

/// How long a timed workshop takes per unit of time at `speed`, from the
/// game's own `duration`, which is the inverse of the game's speed setting.
pub(crate) fn workshop_duration(duration: f32, speed: f32) -> f32 {
    duration / speed.max(MIN_WORKSHOP_SPEED)
}

/// The skill points per level `points` times `factor`, by [`scale_whole`].
pub(crate) fn scale_points(points: u32, factor: f32) -> u32 {
    u32::try_from(scale_whole(i64::from(points), factor).max(0)).unwrap_or(u32::MAX)
}

/// A rolled stack of `count` items times `factor`, by [`scale_whole`],
/// held to 1 up to `max`: a roll that made something still makes one.
pub(crate) fn scale_stack(count: u16, factor: f32, max: u16) -> u16 {
    if count == 0 {
        return 0;
    }
    let scaled = scale_whole(i64::from(count), factor).clamp(1, i64::from(max.max(1)));
    u16::try_from(scaled).unwrap_or(max)
}

/// The comfort `value` times `factor`, by [`scale_whole`], saturated.
pub(crate) fn scale_comfort(value: i32, factor: f32) -> i32 {
    let scaled = scale_whole(i64::from(value), factor);
    i32::try_from(scaled).unwrap_or(if scaled < 0 { i32::MIN } else { i32::MAX })
}

const ONE: u32 = 0x3f80_0000;

/// One mod's declared rate and its factor, as `f32` bits.
#[derive(Debug)]
struct Declared {
    owner: ModIndex,
    rate: RateId,
    factor: AtomicU32,
}

/// Every mod's declared rates: open while mods initialise, sealed once they
/// all have, after which sets and reads neither allocate nor lock.
#[derive(Debug)]
pub(crate) struct Rates {
    open: Mutex<Vec<Declared>>,
    sealed: OnceLock<Box<[Declared]>>,
}

/// The rates of this process.
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game links rates")
)]
pub(crate) static RATES: Rates = Rates::new();

impl Rates {
    pub(crate) const fn new() -> Self {
        Self {
            open: Mutex::new(Vec::new()),
            sealed: OnceLock::new(),
        }
    }

    /// Declares `rate` for mod `owner`, whose build has what it needs when
    /// `available` and which takes part in the join check when `joins`.
    pub(crate) fn declare(
        &self,
        owner: ModIndex,
        rate: RateId,
        available: bool,
        joins: bool,
    ) -> Status {
        if Rate::from_id(rate).is_none() {
            return Status::BAD_ARGS;
        }
        if Rate::from_id(rate).is_some_and(Rate::seen_by_clients) && !joins {
            return Status::REFUSED;
        }
        if !available {
            return Status::MISSING;
        }
        let mut open = self.open.relock();
        if self.sealed.get().is_some() {
            return Status::TOO_LATE;
        }
        if !open.iter().any(|d| d.owner == owner && d.rate == rate) {
            open.push(Declared {
                owner,
                rate,
                factor: AtomicU32::new(ONE),
            });
        }
        Status::OK
    }

    /// Puts `factor` in force for mod `owner`'s declared `rate`.
    pub(crate) fn set(&self, owner: ModIndex, rate: RateId, factor: f32) -> Status {
        if !is_valid_factor(factor) {
            return Status::BAD_ARGS;
        }
        let store = |declared: &[Declared]| {
            declared
                .iter()
                .find(|d| d.owner == owner && d.rate == rate)
                .map_or(Status::BAD_ARGS, |d| {
                    d.factor.store(factor.to_bits(), Ordering::Release);
                    Status::OK
                })
        };
        if let Some(sealed) = self.sealed.get() {
            return store(sealed);
        }
        let open = self.open.relock();
        match self.sealed.get() {
            Some(sealed) => store(sealed),
            None => store(&open),
        }
    }

    /// Ends the declarations: what mods declared stays, read lock-free.
    pub(crate) fn seal(&self) {
        let mut open = self.open.relock();
        let _first = self
            .sealed
            .set(std::mem::take(&mut *open).into_boxed_slice());
    }

    /// Whether any mod declared `rate`; `false` before the seal.
    pub(crate) fn declared(&self, rate: RateId) -> bool {
        self.sealed
            .get()
            .is_some_and(|sealed| sealed.iter().any(|d| d.rate == rate))
    }

    /// The factor in force for `rate`: the factors of the mods that declared
    /// it and are not `off`, combined; 1 before the seal.
    pub(crate) fn factor(&self, rate: RateId, off: impl Fn(ModIndex) -> bool) -> f32 {
        let Some(sealed) = self.sealed.get() else {
            return 1.0;
        };
        combine(
            sealed
                .iter()
                .filter(|d| d.rate == rate && !off(d.owner))
                .map(|d| f32::from_bits(d.factor.load(Ordering::Acquire))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMBAT: RateId = RateId::EXPERIENCE_COMBAT;
    const MINING: RateId = RateId::EXPERIENCE_MINING;

    fn on(_owner: ModIndex) -> bool {
        false
    }

    #[test]
    fn every_rate_of_the_api_is_known_to_the_loader() {
        for rate in Rate::ALL {
            assert!(!links(rate.id()).is_empty(), "{rate:?} has no way to apply");
        }
    }

    #[test]
    fn several_mods_factors_multiply_in_any_order() {
        let rates = Rates::new();
        for owner in [0, 1, 2] {
            assert_eq!(
                rates.declare(ModIndex::new(owner), COMBAT, true, true),
                Status::OK
            );
        }
        assert_eq!(rates.set(ModIndex::new(0), COMBAT, 2.0), Status::OK);
        rates.seal();
        assert_eq!(rates.set(ModIndex::new(2), COMBAT, 1.5), Status::OK);
        assert_eq!(rates.factor(COMBAT, on), 3.0);
        assert_eq!(rates.factor(MINING, on), 1.0);
        assert!(rates.declared(COMBAT) && !rates.declared(MINING));
    }

    #[test]
    fn a_mod_that_is_off_drops_out_of_the_product() {
        let rates = Rates::new();
        rates.declare(ModIndex::new(0), COMBAT, true, true);
        rates.declare(ModIndex::new(1), COMBAT, true, true);
        rates.set(ModIndex::new(0), COMBAT, 4.0);
        rates.set(ModIndex::new(1), COMBAT, 0.5);
        rates.seal();
        assert_eq!(rates.factor(COMBAT, |owner| owner == ModIndex::new(1)), 4.0);
    }

    #[test]
    fn the_product_is_held_to_the_largest_factor() {
        let rates = Rates::new();
        rates.declare(ModIndex::new(0), COMBAT, true, true);
        rates.declare(ModIndex::new(1), COMBAT, true, true);
        rates.set(ModIndex::new(0), COMBAT, 100.0);
        rates.set(ModIndex::new(1), COMBAT, 100.0);
        rates.seal();
        assert_eq!(rates.factor(COMBAT, on), enshwick::rates::MAX_FACTOR);
    }

    #[test]
    fn declarations_are_refused_as_the_abi_says() {
        let rates = Rates::new();
        assert_eq!(
            rates.declare(ModIndex::new(0), RateId(0), true, true),
            Status::BAD_ARGS
        );
        assert_eq!(
            rates.declare(ModIndex::new(0), COMBAT, false, true),
            Status::MISSING
        );
        assert_eq!(
            rates.set(ModIndex::new(0), COMBAT, 2.0),
            Status::BAD_ARGS,
            "undeclared"
        );
        assert_eq!(
            rates.declare(ModIndex::new(0), COMBAT, true, true),
            Status::OK
        );
        assert_eq!(
            rates.declare(ModIndex::new(0), COMBAT, true, true),
            Status::OK,
            "twice"
        );
        for factor in [-1.0, 100.5, f32::NAN, f32::INFINITY] {
            assert_eq!(
                rates.set(ModIndex::new(0), COMBAT, factor),
                Status::BAD_ARGS
            );
        }
        rates.seal();
        assert_eq!(
            rates.declare(ModIndex::new(1), COMBAT, true, true),
            Status::TOO_LATE
        );
        assert_eq!(rates.factor(COMBAT, on), 1.0);
    }

    #[test]
    fn only_a_mod_that_joins_declares_a_rate_clients_count() {
        let rates = Rates::new();
        let speed = RateId::WORKSHOP_SPEED;
        assert_eq!(
            rates.declare(ModIndex::new(0), speed, true, false),
            Status::REFUSED
        );
        assert_eq!(
            rates.declare(ModIndex::new(0), speed, true, true),
            Status::OK
        );
        assert_eq!(
            rates.declare(ModIndex::new(1), COMBAT, true, false),
            Status::OK
        );
    }

    #[test]
    fn nothing_counts_before_the_seal() {
        let rates = Rates::new();
        rates.declare(ModIndex::new(0), COMBAT, true, true);
        rates.set(ModIndex::new(0), COMBAT, 2.0);
        assert_eq!(rates.factor(COMBAT, on), 1.0);
        assert!(!rates.declared(COMBAT));
    }

    #[test]
    fn every_experience_source_counts_under_its_rate_by_its_engine_value() {
        let value = |id: &str| match id.rsplit_once('.').map(|(_, name)| name) {
            Some("combat") => Some(40),
            Some("mining") => Some(41),
            Some("exploration" | "lore" | "quest" | "item_set_completed") => Some(50),
            Some("none") => Some(0),
            _ => None,
        };
        let rates = experience_rates(value);
        assert_eq!(rates[40], Some(RateId::EXPERIENCE_COMBAT));
        assert_eq!(rates[41], Some(RateId::EXPERIENCE_MINING));
        assert_eq!(rates[50], Some(RateId::EXPERIENCE_EXPLORATION));
        assert_eq!(rates[0], None, "no factor without a source");
        assert_eq!(rates[4], None, "a value no source has");
    }

    #[test]
    fn kill_experience_scales_rounds_and_saturates() {
        assert_eq!(scale_whole(50, 2.0), 100);
        assert_eq!(scale_whole(3, 1.5), 5);
        assert_eq!(scale_whole(7, 0.0), 0);
        assert_eq!(scale_whole(i64::MAX, 100.0), i64::MAX);
    }

    #[test]
    fn whole_numbers_scale_round_and_saturate() {
        assert_eq!(scale_points(2, 1.5), 3);
        assert_eq!(scale_points(2, 0.0), 0);
        assert_eq!(scale_points(u32::MAX, 100.0), u32::MAX);
        assert_eq!(scale_comfort(3, 1.5), 5);
        assert_eq!(scale_comfort(-3, 1.5), -5);
        assert_eq!(scale_comfort(i32::MAX, 2.0), i32::MAX);
        assert_eq!(scale_stack(3, 2.0, 20), 6);
        assert_eq!(scale_stack(15, 2.0, 20), 20);
        assert_eq!(scale_stack(1, 0.0, 20), 1);
        assert_eq!(scale_stack(0, 5.0, 20), 0);
        assert_eq!(scale_stack(4, 2.0, 0), 1);
    }

    #[test]
    fn a_faster_workshop_takes_less_time_and_never_stops() {
        assert_eq!(workshop_duration(1.0, 2.0), 0.5);
        assert_eq!(workshop_duration(0.5, 1.0), 0.5);
        assert_eq!(workshop_duration(1.0, 0.0), 100.0);
        assert_eq!(
            workshop_duration(0.0, 4.0),
            0.0,
            "the game's own zero stays"
        );
    }
}

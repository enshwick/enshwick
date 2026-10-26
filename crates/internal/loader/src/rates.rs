//! Game rates mods multiply: each mod's declared rates and factors, and the
//! factor in force by the API's one combination rule. The loader never
//! writes a factor into the game's settings, which the server saves back to
//! its configuration: it scales what the game's own functions answer.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{RateId, Status};
use enshwick::rates::{Rate, combine, is_valid_factor};

use crate::locks::Relock;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "experience.kill",
    "experience.source_factor",
    "settings.factory_speed",
    "settings.resource_drops",
];

/// For each rate, the build table symbols of the game functions whose
/// answer the loader scales; the rate works where every one resolves.
pub(crate) const LINKS: &[(RateId, &[&str])] = &[
    (
        RateId::EXPERIENCE_COMBAT,
        &["experience.source_factor", "experience.kill"],
    ),
    (RateId::EXPERIENCE_MINING, &["experience.source_factor"]),
    (
        RateId::EXPERIENCE_EXPLORATION,
        &["experience.source_factor"],
    ),
    (RateId::WORKSHOP_SPEED, &["settings.factory_speed"]),
    (RateId::RESOURCE_DROPS, &["settings.resource_drops"]),
];

/// The build table symbols `rate` links; empty for a rate the loader does
/// not know.
pub(crate) fn links(rate: RateId) -> &'static [&'static str] {
    LINKS
        .iter()
        .find(|&&(id, _)| id == rate)
        .map_or(&[], |&(_, symbols)| symbols)
}

/// The rate an experience source of the game counts under, by
/// `keen::ExperienceSourceType` on server 399 and client 426: exploration,
/// lore, quests and item sets share the exploration factor, as the game's
/// own settings do.
pub(crate) fn experience_rate(source: u8) -> Option<RateId> {
    match source {
        1..=3 | 6 => Some(RateId::EXPERIENCE_EXPLORATION),
        4 => Some(RateId::EXPERIENCE_COMBAT),
        5 => Some(RateId::EXPERIENCE_MINING),
        _ => None,
    }
}

/// `count` times `factor`, rounded half away from zero and saturated.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "an experience count is far below 2^53, and the result is clamped first"
)]
pub(crate) fn scale_count(count: i64, factor: f32) -> i64 {
    let scaled = (count as f64 * f64::from(factor)).round();
    scaled.clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

const ONE: u32 = 0x3f80_0000;

/// One mod's declared rate and its factor, as `f32` bits.
#[derive(Debug)]
struct Declared {
    owner: usize,
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
        owner: usize,
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
    pub(crate) fn set(&self, owner: usize, rate: RateId, factor: f32) -> Status {
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
    pub(crate) fn factor(&self, rate: RateId, off: impl Fn(usize) -> bool) -> f32 {
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

    fn on(_owner: usize) -> bool {
        false
    }

    #[test]
    fn every_rate_of_the_api_is_known_to_the_loader() {
        for rate in Rate::ALL {
            assert!(!links(rate.id()).is_empty(), "{rate:?} has no way to apply");
        }
    }

    #[test]
    fn every_linked_symbol_has_its_table_id() {
        for (rate, symbols) in LINKS {
            for symbol in *symbols {
                assert!(TABLE_IDS.contains(symbol), "{rate:?}: {symbol}");
            }
        }
    }

    #[test]
    fn several_mods_factors_multiply_in_any_order() {
        let rates = Rates::new();
        for owner in [0, 1, 2] {
            assert_eq!(rates.declare(owner, COMBAT, true, true), Status::OK);
        }
        assert_eq!(rates.set(0, COMBAT, 2.0), Status::OK);
        rates.seal();
        assert_eq!(rates.set(2, COMBAT, 1.5), Status::OK);
        assert_eq!(rates.factor(COMBAT, on), 3.0);
        assert_eq!(rates.factor(MINING, on), 1.0);
        assert!(rates.declared(COMBAT) && !rates.declared(MINING));
    }

    #[test]
    fn a_mod_that_is_off_drops_out_of_the_product() {
        let rates = Rates::new();
        rates.declare(0, COMBAT, true, true);
        rates.declare(1, COMBAT, true, true);
        rates.set(0, COMBAT, 4.0);
        rates.set(1, COMBAT, 0.5);
        rates.seal();
        assert_eq!(rates.factor(COMBAT, |owner| owner == 1), 4.0);
    }

    #[test]
    fn the_product_is_held_to_the_largest_factor() {
        let rates = Rates::new();
        rates.declare(0, COMBAT, true, true);
        rates.declare(1, COMBAT, true, true);
        rates.set(0, COMBAT, 100.0);
        rates.set(1, COMBAT, 100.0);
        rates.seal();
        assert_eq!(rates.factor(COMBAT, on), enshwick::rates::MAX_FACTOR);
    }

    #[test]
    fn declarations_are_refused_as_the_abi_says() {
        let rates = Rates::new();
        assert_eq!(rates.declare(0, RateId(0), true, true), Status::BAD_ARGS);
        assert_eq!(rates.declare(0, COMBAT, false, true), Status::MISSING);
        assert_eq!(rates.set(0, COMBAT, 2.0), Status::BAD_ARGS, "undeclared");
        assert_eq!(rates.declare(0, COMBAT, true, true), Status::OK);
        assert_eq!(rates.declare(0, COMBAT, true, true), Status::OK, "twice");
        for factor in [-1.0, 100.5, f32::NAN, f32::INFINITY] {
            assert_eq!(rates.set(0, COMBAT, factor), Status::BAD_ARGS);
        }
        rates.seal();
        assert_eq!(rates.declare(1, COMBAT, true, true), Status::TOO_LATE);
        assert_eq!(rates.factor(COMBAT, on), 1.0);
    }

    #[test]
    fn only_a_mod_that_joins_declares_a_rate_clients_count() {
        let rates = Rates::new();
        let speed = RateId::WORKSHOP_SPEED;
        assert_eq!(rates.declare(0, speed, true, false), Status::REFUSED);
        assert_eq!(rates.declare(0, speed, true, true), Status::OK);
        assert_eq!(rates.declare(1, COMBAT, true, false), Status::OK);
    }

    #[test]
    fn nothing_counts_before_the_seal() {
        let rates = Rates::new();
        rates.declare(0, COMBAT, true, true);
        rates.set(0, COMBAT, 2.0);
        assert_eq!(rates.factor(COMBAT, on), 1.0);
        assert!(!rates.declared(COMBAT));
    }

    #[test]
    fn every_experience_source_counts_under_its_rate() {
        assert_eq!(experience_rate(4), Some(RateId::EXPERIENCE_COMBAT));
        assert_eq!(experience_rate(5), Some(RateId::EXPERIENCE_MINING));
        for source in [1, 2, 3, 6] {
            assert_eq!(
                experience_rate(source),
                Some(RateId::EXPERIENCE_EXPLORATION)
            );
        }
        assert_eq!(experience_rate(0), None);
        assert_eq!(experience_rate(7), None);
    }

    #[test]
    fn kill_experience_scales_rounds_and_saturates() {
        assert_eq!(scale_count(50, 2.0), 100);
        assert_eq!(scale_count(3, 1.5), 5);
        assert_eq!(scale_count(7, 0.0), 0);
        assert_eq!(scale_count(i64::MAX, 100.0), i64::MAX);
    }
}

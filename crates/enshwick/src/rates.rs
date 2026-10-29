//! Game rates: multipliers on what the game hands out, such as experience.
//!
//! A mod declares each [`Rate`] with [`Load::rate`] at init and sets its
//! factor through the [`RateHandle`]; the game uses the product of every
//! declaring mod's factor ([`combine`]).
//!
//! # Examples
//!
//! ```rust,standalone_crate
//! use enshwick::rates::{ExperienceGroup, Rate};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let combat = load.rate(Rate::Experience(ExperienceGroup::Combat))?;
//!     combat.set(2.0)?;
//!     Ok(())
//! }
//! ```

use crate::Host;
use crate::abi::{RateId, Status};
use crate::content::recipes::RecipeKind;
#[cfg(feature = "unstable")]
use crate::{Load, Off};

/// The largest factor a mod sets and the largest product in force.
pub const MAX_FACTOR: f32 = 100.0;

/// The groups of experience sources the game's settings give one factor
/// each, and so one rate each.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExperienceGroup {
    /// Defeated enemies.
    Combat,
    /// Mining.
    Mining,
    /// Exploration, lore, quests and completed item sets.
    ExplorationQuests,
}

impl ExperienceGroup {
    /// The group an experience source counts in; `None` for a source the
    /// game gives no factor.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::game_events::ExperienceSourceType;
    /// use enshwick::rates::ExperienceGroup;
    ///
    /// assert_eq!(ExperienceGroup::of(ExperienceSourceType::Quest), Some(ExperienceGroup::ExplorationQuests));
    /// assert_eq!(ExperienceGroup::of(ExperienceSourceType::None), None);
    /// ```
    #[must_use]
    pub const fn of(source: crate::game_events::ExperienceSourceType) -> Option<Self> {
        use crate::game_events::ExperienceSourceType as Source;
        match source {
            Source::Combat => Some(Self::Combat),
            Source::Mining => Some(Self::Mining),
            Source::Exploration | Source::Lore | Source::Quest | Source::ItemSetCompleted => {
                Some(Self::ExplorationQuests)
            }
            _ => None,
        }
    }
}

/// What a rate multiplies.
///
/// The type's name is enshwick's. A rate the game has a setting for takes
/// that setting's name; the others (`Comfort`, `SkillPoints`, `Loot`,
/// `RecipeOutput`) are enshwick's.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::rates::Rate;
///
/// assert_eq!(Rate::from_id(Rate::FactoryProductionSpeed.id()), Some(Rate::FactoryProductionSpeed));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Rate {
    /// Experience from one source.
    Experience(ExperienceGroup),
    /// How fast timed workshops produce: 2 halves the time an item takes,
    /// and a factor below 0.01 counts as 0.01, so a workshop never stops.
    /// Clients count it too, for the time their screens show, so only a
    /// mod with `join: true` declares it and sets it from a synced setting:
    /// every player on a server then runs the mod with the server's factor.
    FactoryProductionSpeed,
    /// How many resources a drop stacks, as the game's own setting does.
    ResourceDropStackAmount,
    /// The comfort each piece of furniture gives a base, rounded to a
    /// whole number; the server decides comfort.
    Comfort,
    /// The skill points a character gains per level, rounded to a whole
    /// number; points from knowledge stay as they are. Clients count it
    /// too, for the points their skill tree shows, and the server checks
    /// every unlock against its own count, so only a mod with
    /// `join: true` declares it, set from a synced setting. Lowering it
    /// mid-game leaves a character that spent more than its new total
    /// unable to unlock more skills until it resets its tree.
    SkillPoints,
    /// How many items each stack of random loot holds, rounded and held
    /// to the item's stack size: the loot of chests, corpses and animals and
    /// random pickups. Where the game's resource drop setting also scales a
    /// roll, both apply.
    Loot,
    /// How many items a recipe makes, for the vanilla and mod recipes whose
    /// output is of this kind, rounded and at least one; applied after every
    /// mod's own recipe patch. The crafting menu shows the counts, so only a
    /// mod with `join: true` declares it, set from a synced setting.
    RecipeOutput(RecipeKind),
}

impl Rate {
    /// Every rate that takes no part: each one alone.
    const SINGLE: &[Self] = &[
        Self::Experience(ExperienceGroup::Combat),
        Self::Experience(ExperienceGroup::Mining),
        Self::Experience(ExperienceGroup::ExplorationQuests),
        Self::FactoryProductionSpeed,
        Self::ResourceDropStackAmount,
        Self::Comfort,
        Self::SkillPoints,
        Self::Loot,
    ];

    /// Every rate: those that stand alone, then one recipe output rate per
    /// [`RecipeKind::ALL`].
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::rates::Rate;
    ///
    /// assert!(Rate::all().any(|rate| rate == Rate::FactoryProductionSpeed));
    /// ```
    pub fn all() -> impl Iterator<Item = Self> {
        Self::SINGLE
            .iter()
            .copied()
            .chain(RecipeKind::ALL.iter().map(|&kind| Self::RecipeOutput(kind)))
    }

    /// The loader's number for this rate.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::abi::RateId;
    /// use enshwick::rates::Rate;
    ///
    /// assert_eq!(Rate::FactoryProductionSpeed.id(), RateId::FACTORY_PRODUCTION_SPEED);
    /// ```
    #[must_use]
    pub const fn id(self) -> RateId {
        match self {
            Self::Experience(ExperienceGroup::Combat) => RateId::EXPERIENCE_COMBAT,
            Self::Experience(ExperienceGroup::Mining) => RateId::EXPERIENCE_MINING,
            Self::Experience(ExperienceGroup::ExplorationQuests) => {
                RateId::EXPERIENCE_EXPLORATION_QUESTS
            }
            Self::FactoryProductionSpeed => RateId::FACTORY_PRODUCTION_SPEED,
            Self::ResourceDropStackAmount => RateId::RESOURCE_DROP_STACK_AMOUNT,
            Self::Comfort => RateId::COMFORT,
            Self::SkillPoints => RateId::SKILL_POINTS,
            Self::Loot => RateId::LOOT,
            Self::RecipeOutput(kind) => RateId::recipe_output(kind.code()),
        }
    }

    /// The rate the loader numbers `id`; `None` for a number this API does
    /// not name.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::abi::RateId;
    /// use enshwick::rates::Rate;
    ///
    /// assert_eq!(Rate::from_id(RateId::RESOURCE_DROP_STACK_AMOUNT), Some(Rate::ResourceDropStackAmount));
    /// assert_eq!(Rate::from_id(RateId(0)), None);
    /// ```
    #[must_use]
    pub fn from_id(id: RateId) -> Option<Self> {
        Self::all().find(|rate| rate.id() == id)
    }

    /// Whether clients count the rate too, so only a mod that joins
    /// declares it.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::rates::{ExperienceGroup, Rate};
    ///
    /// assert!(Rate::FactoryProductionSpeed.seen_by_clients() && Rate::SkillPoints.seen_by_clients());
    /// assert!(!Rate::Experience(ExperienceGroup::Combat).seen_by_clients());
    /// ```
    #[must_use]
    pub const fn seen_by_clients(self) -> bool {
        matches!(
            self,
            Self::FactoryProductionSpeed | Self::SkillPoints | Self::RecipeOutput(_)
        )
    }
}

/// Whether `factor` is one a mod may set: finite, from 0 to [`MAX_FACTOR`].
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::rates::is_valid_factor;
///
/// assert!(is_valid_factor(0.0) && is_valid_factor(2.5));
/// assert!(!is_valid_factor(-1.0) && !is_valid_factor(f32::NAN));
/// ```
#[must_use]
pub fn is_valid_factor(factor: f32) -> bool {
    (0.0..=MAX_FACTOR).contains(&factor)
}

/// The factor in force for a rate whose declaring mods set `factors`: their
/// product, held to 0 to [`MAX_FACTOR`]; 1 when there are none.
///
/// The one combination rule of every rate, in the loader and the fake host
/// alike. A product is the same in any order, so load order does not matter.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::rates::combine;
///
/// assert_eq!(combine([2.0, 1.5]), 3.0);
/// assert_eq!(combine([]), 1.0);
/// assert_eq!(combine([50.0, 50.0]), 100.0);
/// ```
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "held to 0..=100 first, which an f32 holds"
)]
pub fn combine(factors: impl IntoIterator<Item = f32>) -> f32 {
    let product = factors
        .into_iter()
        .fold(1.0_f64, |product, factor| product * f64::from(factor));
    product.clamp(0.0, f64::from(MAX_FACTOR)) as f32
}

/// `value` times `factor`, rounded half away from zero and saturated: how
/// the loader scales a whole number the game keeps, such as a comfort value,
/// skill points or a kill's experience.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::rates::scale_whole;
///
/// assert_eq!(scale_whole(3, 1.5), 5);
/// assert_eq!(scale_whole(-3, 1.5), -5);
/// assert_eq!(scale_whole(i64::MAX, 2.0), i64::MAX);
/// ```
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "the game's whole numbers are far below 2^53, and the result is clamped first"
)]
pub fn scale_whole(value: i64, factor: f32) -> i64 {
    let scaled = (value as f64 * f64::from(factor)).round();
    scaled.clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

/// Why a factor did not take.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::rates::RateError;
///
/// assert_eq!(RateError::OutOfRange(-1.0).to_string(), "factor -1 is not between 0 and 100");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RateError {
    /// The factor is not finite or lies outside 0 to [`MAX_FACTOR`].
    #[error("factor {0} is not between 0 and 100")]
    OutOfRange(f32),
    /// The loader refused the factor, as for a mod that is off.
    #[error("the loader refused the factor")]
    Refused,
}

/// A rate the mod declared; sets the mod's factor for it.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct RateHandle {
    host: Host,
    rate: Rate,
}

impl RateHandle {
    /// The rate this handle sets.
    ///
    /// # Examples
    ///
    /// ```rust,standalone_crate
    /// use enshwick::rates::{Rate, RateHandle};
    ///
    /// fn is_speed(handle: &RateHandle) -> bool {
    ///     handle.rate() == Rate::FactoryProductionSpeed
    /// }
    /// ```
    #[must_use]
    pub const fn rate(&self) -> Rate {
        self.rate
    }

    /// Puts `factor` in force as this mod's factor for the rate: 1 is
    /// vanilla, 2 doubles, 0 takes it all, except where a rate keeps a
    /// least value ([`Rate::FactoryProductionSpeed`], [`Rate::Loot`]). The game uses it from the next
    /// time it reads the rate. Any thread; neither allocates nor locks once
    /// every mod is initialised.
    ///
    /// # Errors
    ///
    /// [`RateError::OutOfRange`] for a factor [`is_valid_factor`] refuses,
    /// [`RateError::Refused`] when the loader refuses it.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set(&self, factor: f32) -> Result<(), RateError> {
        if !is_valid_factor(factor) {
            return Err(RateError::OutOfRange(factor));
        }
        match (self.host.api().set_rate)(self.host.handle(), self.rate.id(), factor) {
            Status::OK => Ok(()),
            _ => Err(RateError::Refused),
        }
    }
}

#[cfg(feature = "unstable")]
impl Load<'_> {
    /// Declares that this mod multiplies `rate` and hands back the handle
    /// that sets its factor, 1 until set. The loader scales the game's
    /// answers in every process the mod runs in; what the server decides,
    /// such as experience and loot, counts on the dedicated server and on a
    /// client while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build that lacks what the rate needs, and for a rate
    /// [`seen by clients`](Rate::seen_by_clients) in a mod without
    /// `join: true`; the rest of the mod keeps running when it carries on
    /// without this rate.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn rate(&mut self, rate: Rate) -> Result<RateHandle, Off> {
        if rate.seen_by_clients() && !self.joins() {
            return Err(Off::new(format!(
                "the rate {rate:?}: clients count it too, so only a mod with `join: true` declares it"
            )));
        }
        let host = self.host();
        match (host.api().rate)(host.handle(), rate.id()) {
            Status::OK => Ok(RateHandle { host, rate }),
            status => Err(Off::status(&format!("the rate {rate:?}"), status)),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn every_recipe_kind_has_its_output_rate_and_ids_are_unique() {
        let all: Vec<Rate> = Rate::all().collect();
        for &kind in RecipeKind::ALL {
            assert!(all.contains(&Rate::RecipeOutput(kind)), "{kind:?}");
        }
        for (i, a) in all.iter().enumerate() {
            assert_eq!(Rate::from_id(a.id()), Some(*a));
            assert!(all[i + 1..].iter().all(|b| b.id() != a.id()), "{a:?}");
        }
    }

    proptest! {
        #[test]
        fn combine_is_the_same_in_any_order_and_stays_in_range(
            factors in proptest::collection::vec(0.0f32..=MAX_FACTOR, 0..6),
        ) {
            let forward = combine(factors.iter().copied());
            let backward = combine(factors.iter().rev().copied());
            prop_assert_eq!(forward.to_bits(), backward.to_bits());
            prop_assert!((0.0..=MAX_FACTOR).contains(&forward));
        }

        #[test]
        fn scale_whole_never_flips_the_sign(value in any::<i64>(), factor in 0.0f32..=MAX_FACTOR) {
            let scaled = scale_whole(value, factor);
            prop_assert!(scaled == 0 || scaled.signum() == value.signum());
        }
    }
}

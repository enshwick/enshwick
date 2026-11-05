//! Weather phenomena of a mod's own and changes to the game's, served in the
//! game's weather resource.

use core::time::Duration;

use super::WeatherPhenomenonEffect;
use crate::abi::Level;
use crate::content::{Guid, Id};
use crate::resources::Registry;
use crate::typed::{
    ResourceType, Time, WeatherPhenomenonEffect as GameEffect, WeatherSystemPhenomenonResource,
    WeatherSystemResource,
};
use crate::{Load, Off};

/// A weather phenomenon a mod added, by the GUID its content id maps to.
///
/// # Examples
///
/// See [`NewPhenomenon`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct PhenomenonId(pub Guid);

/// Values a mod sets on a weather phenomenon; each `None` keeps the value
/// of the phenomenon it starts from.
///
/// # Examples
///
/// See [`NewPhenomenon`].
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct PhenomenonChange {
    /// How likely the game picks it in its layer, 0 to 1.
    #[script(default)]
    pub probability: Option<f32>,
    /// The shortest it lasts, in game time.
    #[script(default)]
    pub duration_min: Option<Duration>,
    /// The longest it lasts, in game time.
    #[script(default)]
    pub duration_max: Option<Duration>,
    /// The smallest radius, in metres.
    #[script(default)]
    pub radius_min: Option<f32>,
    /// The largest radius, in metres.
    #[script(default)]
    pub radius_max: Option<f32>,
    /// How strong it is.
    #[script(default)]
    pub strength: Option<f32>,
    /// What it brings: clouds alone, rain or a blizzard.
    #[script(default)]
    pub effect: Option<WeatherPhenomenonEffect>,
}

impl PhenomenonChange {
    /// No value changed.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            probability: None,
            duration_min: None,
            duration_max: None,
            radius_min: None,
            radius_max: None,
            strength: None,
            effect: None,
        }
    }

    /// Sets how likely the game picks it in its layer.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn probability(mut self, probability: f32) -> Self {
        self.probability = Some(probability);
        self
    }

    /// Sets the shortest and the longest it lasts.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn duration(mut self, min: Duration, max: Duration) -> Self {
        self.duration_min = Some(min);
        self.duration_max = Some(max);
        self
    }

    /// Sets the smallest and the largest radius, in metres.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn radius(mut self, min: f32, max: f32) -> Self {
        self.radius_min = Some(min);
        self.radius_max = Some(max);
        self
    }

    /// Sets how strong it is.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn strength(mut self, strength: f32) -> Self {
        self.strength = Some(strength);
        self
    }

    /// Sets what it brings.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn effect(mut self, effect: WeatherPhenomenonEffect) -> Self {
        self.effect = Some(effect);
        self
    }

    /// Why the values cannot stand, if they cannot.
    fn check(&self) -> Result<(), PhenomenonError> {
        let number = |v: Option<f32>| v.is_none_or(|v| v.is_finite() && v >= 0.0);
        let ordered = |min: Option<f32>, max: Option<f32>| match (min, max) {
            (Some(min), Some(max)) => min <= max,
            _ => true,
        };
        let fits = number(self.probability)
            && self.probability.is_none_or(|p| p <= 1.0)
            && number(self.radius_min)
            && number(self.radius_max)
            && number(self.strength)
            && ordered(self.radius_min, self.radius_max)
            && match (self.duration_min, self.duration_max) {
                (Some(min), Some(max)) => min <= max,
                _ => true,
            };
        if !fits {
            return Err(PhenomenonError::Values);
        }
        if matches!(self.effect, Some(WeatherPhenomenonEffect::Other(_))) {
            return Err(PhenomenonError::Effect);
        }
        Ok(())
    }

    /// Writes the values set into `phenomenon`.
    fn apply(&self, phenomenon: &mut WeatherSystemPhenomenonResource) {
        let time = |d: Duration| Time {
            value: i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        };
        if let Some(p) = self.probability {
            phenomenon.probability = p;
        }
        if let Some(d) = self.duration_min {
            phenomenon.duration_min = time(d);
        }
        if let Some(d) = self.duration_max {
            phenomenon.duration_max = time(d);
        }
        if let Some(r) = self.radius_min {
            phenomenon.radius_min = r;
        }
        if let Some(r) = self.radius_max {
            phenomenon.radius_max = r;
        }
        if let Some(s) = self.strength {
            phenomenon.strength = s;
        }
        phenomenon.effect = match self.effect {
            None | Some(WeatherPhenomenonEffect::Other(_)) => return,
            Some(WeatherPhenomenonEffect::None) => GameEffect::None,
            Some(WeatherPhenomenonEffect::Rain) => GameEffect::Rain,
            Some(WeatherPhenomenonEffect::Blizzard) => GameEffect::Blizzard,
        };
    }
}

/// A weather phenomenon of a mod's own: a copy of one of the game's, its
/// clouds and look included, with values changed, in a weather layer.
///
/// The game knows only clouds, rain and blizzards, so a phenomenon picks
/// one of them; a new look needs client assets and is not offered. Every
/// client draws weather from its own copy of the resource, so a mod that
/// adds one runs on clients too.
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::content::{Guid, Id};
/// use enshwick::weather::{NewPhenomenon, PhenomenonChange, WeatherPhenomenonEffect};
///
/// fn drizzle(load: &mut enshwick::Load<'_>, rain: Guid) -> Result<(), enshwick::Off> {
///     let values = PhenomenonChange::new()
///         .probability(0.5)
///         .duration(Duration::from_mins(2), Duration::from_mins(5))
///         .effect(WeatherPhenomenonEffect::Rain);
///     load.add_weather_phenomenon(NewPhenomenon::new(Id::new("you.drizzle:drizzle"), rain).values(values))?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct NewPhenomenon {
    /// Its content id, `mod:name`.
    pub id: Id,
    /// The game's phenomenon it copies.
    pub like: Guid,
    /// The weather layer it joins, by its GUID; `None` for the layer of
    /// `like`.
    #[script(default)]
    pub layer: Option<Guid>,
    /// The values it changes from `like`'s.
    #[script(default)]
    pub values: PhenomenonChange,
}

impl NewPhenomenon {
    /// A copy of the game's phenomenon `like` under the content id `id`, in
    /// `like`'s layer.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn new(id: Id, like: Guid) -> Self {
        Self {
            id,
            like,
            layer: None,
            values: PhenomenonChange::new(),
        }
    }

    /// Puts it in the weather layer `layer` instead.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn in_layer(mut self, layer: Guid) -> Self {
        self.layer = Some(layer);
        self
    }

    /// Changes `values` from `like`'s.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[must_use]
    pub const fn values(mut self, values: PhenomenonChange) -> Self {
        self.values = values;
        self
    }
}

/// Why a phenomenon was not added or changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum PhenomenonError {
    #[error("the phenomenon it names is not in the weather resource")]
    Unknown,
    #[error("the weather layer it names is not in the weather resource")]
    Layer,
    #[error("the phenomenon it copies is in no weather layer, so it needs one named")]
    NoLayer,
    #[error("its id is taken by another phenomenon")]
    Taken,
    #[error("the weather resource holds 255 phenomena, its most")]
    Full,
    #[error("another weather layer's range covers where it would go")]
    Split,
    #[error("a probability over 1, a negative or unordered value, or one that is not a number")]
    Values,
    #[error("the game knows clouds, rain and blizzards only")]
    Effect,
}

/// Puts one more phenomenon at the end of layer `target`'s range: the
/// index it goes to, with every other layer at or after it moved up one.
/// Layer ranges are `(start, count)` into one array of phenomena, which may
/// hold phenomena of no layer between them (server 387: three debug ones).
fn place(ranges: &mut [(u8, u8)], target: usize, len: usize) -> Result<usize, PhenomenonError> {
    let (start, count) = *ranges.get(target).ok_or(PhenomenonError::Layer)?;
    let at = usize::from(start) + usize::from(count);
    if len >= usize::from(u8::MAX) || count == u8::MAX {
        return Err(PhenomenonError::Full);
    }
    let covers = |&(start, count): &(u8, u8)| {
        usize::from(start) < at && at < usize::from(start) + usize::from(count)
    };
    if ranges
        .iter()
        .enumerate()
        .any(|(i, r)| i != target && covers(r))
    {
        return Err(PhenomenonError::Split);
    }
    for (i, (start, count)) in ranges.iter_mut().enumerate() {
        if i == target {
            *count += 1;
        } else if usize::from(*start) >= at {
            // `len` < 255 and `start` <= `len`, checked first.
            *start += 1;
        }
    }
    Ok(at)
}

/// Adds the copy `new` of `like` with GUID `guid` to `system`.
fn add(
    system: &mut WeatherSystemResource,
    guid: Guid,
    new: &NewPhenomenon,
) -> Result<(), PhenomenonError> {
    let phenomena = &mut system.phenomenons;
    if phenomena.iter().any(|p| p.guid == guid) {
        return Err(PhenomenonError::Taken);
    }
    let like = phenomena
        .iter()
        .position(|p| p.guid == new.like)
        .ok_or(PhenomenonError::Unknown)?;
    let mut ranges: Vec<(u8, u8)> = system
        .layers
        .iter()
        .map(|l| {
            (
                l.phenomenon_index_range.start,
                l.phenomenon_index_range.count,
            )
        })
        .collect();
    let target = match new.layer {
        Some(layer) => system
            .layers
            .iter()
            .position(|l| l.guid == layer)
            .ok_or(PhenomenonError::Layer)?,
        None => ranges
            .iter()
            .position(|&(start, count)| {
                (usize::from(start)..usize::from(start) + usize::from(count)).contains(&like)
            })
            .ok_or(PhenomenonError::NoLayer)?,
    };
    let at = place(&mut ranges, target, phenomena.len())?;
    let mut copy = phenomena[like].clone();
    copy.guid = guid;
    copy.debug_name = new.id.to_string();
    new.values.apply(&mut copy);
    phenomena.insert(at, copy);
    for (layer, (start, count)) in system.layers.iter_mut().zip(ranges) {
        layer.phenomenon_index_range.start = start;
        layer.phenomenon_index_range.count = count;
    }
    Ok(())
}

/// Changes the phenomenon `guid` of `system`.
fn change(
    system: &mut WeatherSystemResource,
    guid: Guid,
    values: &PhenomenonChange,
) -> Result<(), PhenomenonError> {
    let phenomenon = system
        .phenomenons
        .iter_mut()
        .find(|p| p.guid == guid)
        .ok_or(PhenomenonError::Unknown)?;
    values.apply(phenomenon);
    Ok(())
}

/// The game's weather resource type and the key of its one resource.
fn weather_system(
    load: &Load<'_>,
) -> Result<(ResourceType<WeatherSystemResource>, crate::abi::ResourceKey), Off> {
    let ty = ResourceType::<WeatherSystemResource>::resolve(load)?;
    let hash = ty.key(Guid([0; 16])).type_hash;
    let registry = Registry::game()?;
    let keys = load.host().resource_keys(registry.hashed(hash))?;
    let key = keys
        .into_iter()
        .find(|k| k.type_hash == hash)
        .ok_or_else(|| Off::new("weather phenomena: the game has no weather resource"))?;
    Ok((ty, key))
}

#[crate::script(weather_phenomena)]
impl Load<'_> {
    /// Adds the weather phenomenon `new` to the game's weather, where the
    /// game picks it in its layer as it picks its own; mods that add
    /// phenomena apply in load order, each on the one before's result.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, without the weather resource on this build, and
    /// for a phenomenon or layer the game's weather lacks, an id taken, a
    /// layer full, a value out of bounds or an effect the game does not
    /// know. A collision with another mod's phenomenon is logged and that
    /// mod's is kept.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[script]
    pub fn add_weather_phenomenon(&mut self, new: NewPhenomenon) -> Result<PhenomenonId, Off> {
        let what = |e: PhenomenonError| Off::new(format!("weather phenomenon {}: {e}", new.id));
        new.values.check().map_err(what)?;
        let (ty, key) = weather_system(self)?;
        let guid = new.id.guid();
        add(&mut ty.game_value(key)?, guid, &new).map_err(what)?;
        let host = self.host();
        ty.serve(self, key, key, move |system| {
            if let Err(e) = add(system, guid, &new) {
                host.log(
                    Level::WARN,
                    &format!("weather phenomenon {} not added: {e}", new.id),
                );
            }
        })?;
        Ok(PhenomenonId(guid))
    }

    /// Changes the values of the game's weather phenomenon `phenomenon`, or
    /// of one a mod added before; changes apply in load order.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, without the weather resource on this build, and
    /// for a value out of bounds or an effect the game does not know. A
    /// phenomenon no mod before added and the game lacks is logged and
    /// left out.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::weather::PhenomenonChange;
    ///
    /// fn rarer(load: &mut enshwick::Load<'_>, rain: Guid) -> Result<(), enshwick::Off> {
    ///     load.change_weather_phenomenon(rain, PhenomenonChange::new().probability(0.1))
    /// }
    /// ```
    #[script]
    pub fn change_weather_phenomenon(
        &mut self,
        phenomenon: Guid,
        values: PhenomenonChange,
    ) -> Result<(), Off> {
        values
            .check()
            .map_err(|e| Off::new(format!("weather phenomenon {phenomenon}: {e}")))?;
        let (ty, key) = weather_system(self)?;
        let host = self.host();
        ty.serve(self, key, key, move |system| {
            if let Err(e) = change(system, phenomenon, &values) {
                host.log(
                    Level::WARN,
                    &format!("weather phenomenon {phenomenon} not changed: {e}"),
                );
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layer ranges of the weather resource on server 387.
    const VANILLA: [(u8, u8); 9] = [
        (0, 4),
        (4, 1),
        (5, 1),
        (9, 1),
        (10, 0),
        (10, 1),
        (11, 1),
        (12, 1),
        (13, 0),
    ];

    #[test]
    fn a_phenomenon_goes_at_the_end_of_its_layer_and_moves_the_layers_after_it() {
        let mut ranges = VANILLA;
        assert_eq!(place(&mut ranges, 2, 13), Ok(6));
        assert_eq!(ranges[2], (5, 2));
        assert_eq!(&ranges[..2], &VANILLA[..2], "the layers before stay");
        assert_eq!(ranges[3], (10, 1), "the debug ones between move too");
        assert_eq!(ranges[8], (14, 0));
    }

    #[test]
    fn two_mods_adding_to_one_layer_keep_every_range_apart() {
        let mut ranges = VANILLA;
        assert_eq!(place(&mut ranges, 0, 13), Ok(4));
        assert_eq!(place(&mut ranges, 0, 14), Ok(5));
        assert_eq!(place(&mut ranges, 5, 15), Ok(13));
        assert_eq!(ranges[0], (0, 6));
        assert_eq!(ranges[1], (6, 1));
        assert_eq!(ranges[4], (12, 0), "an empty layer before the place stays");
        assert_eq!(ranges[5], (12, 2));
        assert_eq!(ranges[6], (14, 1));
    }

    #[test]
    fn an_empty_layer_takes_its_first_phenomenon_and_the_layer_at_its_start_moves() {
        let mut ranges = VANILLA;
        assert_eq!(place(&mut ranges, 4, 13), Ok(10));
        assert_eq!(ranges[4], (10, 1));
        assert_eq!(ranges[5], (11, 1));
    }

    #[test]
    fn a_full_resource_an_unknown_layer_and_a_split_range_are_refused() {
        let mut ranges = VANILLA;
        assert_eq!(place(&mut ranges, 0, 255), Err(PhenomenonError::Full));
        assert_eq!(place(&mut ranges, 9, 13), Err(PhenomenonError::Layer));
        let mut overlapping = [(0, 2), (1, 3)];
        assert_eq!(place(&mut overlapping, 0, 4), Err(PhenomenonError::Split));
        assert_eq!(ranges, VANILLA, "a refusal changes nothing");
    }

    #[test]
    fn values_out_of_bounds_and_an_unknown_effect_are_refused() {
        let ok = PhenomenonChange::new()
            .probability(1.0)
            .radius(10.0, 20.0)
            .duration(Duration::from_secs(1), Duration::from_secs(2));
        assert_eq!(ok.check(), Ok(()));
        for bad in [
            PhenomenonChange::new().probability(1.5),
            PhenomenonChange::new().strength(f32::NAN),
            PhenomenonChange::new().radius(20.0, 10.0),
            PhenomenonChange::new().duration(Duration::from_secs(2), Duration::from_secs(1)),
        ] {
            assert_eq!(bad.check(), Err(PhenomenonError::Values), "{bad:?}");
        }
        let other = PhenomenonChange::new().effect(WeatherPhenomenonEffect::Other(7));
        assert_eq!(other.check(), Err(PhenomenonError::Effect));
    }
}

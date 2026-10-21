//! The server's game settings: the difficulty and gameplay values of its
//! configuration (`gameSettings` in `enshrouded_server.json`) or of the
//! host's world menu, read typed and changed while the world runs.
//!
//! Every system reads the live settings when it runs, so a change takes
//! effect from the next step and every client gets it. Some values act on
//! what comes next: enemy health and damage on enemies spawned after the
//! change, food buffs on food eaten after it, the tombstone mode on the next
//! death. A [`change`](GameSettings::change) lasts until the server stops;
//! the configuration's values return with the next start.
//! [`keep`](GameSettings::keep) changes them the same way and keeps the
//! fields it changed with the world: the loader applies them again each time
//! that world loads, over what the configuration says, until
//! [`forget_kept`](GameSettings::forget_kept).
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or a command; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let settings = load.game_settings()?;
//!     load.add_command("hard", "doubles the enemies' damage", move |_ctx, _args| {
//!         match settings.change(|s| s.enemy_damage *= 2.0) {
//!             Ok(now) => format!("enemy damage now {}", now.enemy_damage).into(),
//!             Err(why) => format!("unchanged: {why}").into(),
//!         }
//!     })
//! }
//! ```

use core::time::Duration;

use crate::world::WorldError;
use crate::{Host, Load, Off};

/// The reflected type the settings are.
const TYPE: &str = "keen::ecs::GameSettings";

macro_rules! game_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $value:literal,)* }) => {
        $(#[$doc])*
        ///
        /// # Examples
        ///
        /// See [`Settings`].
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vdoc])* $variant,)*
            /// A value this API does not name, kept as it is.
            Other(u8),
        }

        impl $name {
            /// The variant for the game's byte `value`.
            ///
            /// # Examples
            ///
            /// ```
            /// use enshwick_loader_api::game_settings::WeatherFrequency;
            ///
            /// assert_eq!(WeatherFrequency::from_value(9), WeatherFrequency::Other(9));
            /// ```
            #[must_use]
            pub const fn from_value(value: u8) -> Self {
                match value {
                    $($value => Self::$variant,)*
                    other => Self::Other(other),
                }
            }

            /// The game's byte for this variant.
            ///
            /// # Examples
            ///
            /// ```
            /// use enshwick_loader_api::game_settings::WeatherFrequency;
            ///
            /// assert_eq!(WeatherFrequency::Often.value(), 3);
            /// ```
            #[must_use]
            pub const fn value(self) -> u8 {
                match self {
                    $(Self::$variant => $value,)*
                    Self::Other(other) => other,
                }
            }
        }
    };
}

game_enum! {
    /// The preset the server's settings came from.
    Preset {
        /// The game's defaults.
        Default = 0,
        /// Easier.
        Relaxed = 1,
        /// Harder.
        Hard = 2,
        /// Hardest, with hunger.
        Survival = 3,
        /// Values of their own; what every change makes the preset.
        Custom = 4,
    }
}

game_enum! {
    /// What a dead character leaves behind.
    TombstoneMode {
        /// The backpack's materials go into the tombstone.
        AddBackpackMaterials = 0,
        /// Everything the character carried goes into it.
        Everything = 1,
        /// No tombstone: the character keeps everything.
        NoTombstone = 2,
    }
}

game_enum! {
    /// How often rain and storms come.
    WeatherFrequency {
        /// Never.
        Disabled = 0,
        /// Half as often.
        Rare = 1,
        /// As the game means it.
        Normal = 2,
        /// Twice as often.
        Often = 3,
    }
}

game_enum! {
    /// A difficulty from very easy to very hard, such as fishing's.
    Difficulty {
        /// Very easy.
        VeryEasy = 0,
        /// Easy.
        Easy = 1,
        /// Normal.
        Normal = 2,
        /// Hard.
        Hard = 3,
        /// Very hard.
        VeryHard = 4,
    }
}

game_enum! {
    /// How many enemies random spawners bring, or how many enemies attack at
    /// once.
    Amount {
        /// Few.
        Few = 0,
        /// Normal.
        Normal = 1,
        /// Many.
        Many = 2,
        /// Extreme.
        Extreme = 3,
        /// By the number of players; random spawners only.
        Automatic = 4,
    }
}

game_enum! {
    /// What startling a tamed animal costs its taming.
    StartleRepercussion {
        /// Nothing.
        KeepProgress = 0,
        /// Some of the progress.
        LoseSomeProgress = 1,
        /// All of it.
        LoseAllProgress = 2,
    }
}

game_enum! {
    /// How hard the shroud's curse hits.
    CurseModifier {
        /// Easy.
        Easy = 0,
        /// Normal.
        Normal = 1,
        /// Hard.
        Hard = 2,
    }
}

/// How a field is stored.
trait Value: Sized {
    const SIZE: usize;
    fn decode(bytes: &[u8]) -> Self;
    fn encode(&self, out: &mut [u8]);
    /// The value, when it is a factor.
    fn factor(&self) -> Option<f32> {
        None
    }
}

impl Value for f32 {
    const SIZE: usize = 4;
    fn decode(bytes: &[u8]) -> Self {
        bytes
            .try_into()
            .map(Self::from_le_bytes)
            .unwrap_or_default()
    }
    fn encode(&self, out: &mut [u8]) {
        out.copy_from_slice(&self.to_le_bytes());
    }
    fn factor(&self) -> Option<f32> {
        Some(*self)
    }
}

impl Value for bool {
    const SIZE: usize = 1;
    fn decode(bytes: &[u8]) -> Self {
        bytes.first().is_some_and(|&b| b != 0)
    }
    fn encode(&self, out: &mut [u8]) {
        out[0] = u8::from(*self);
    }
}

impl Value for u8 {
    const SIZE: usize = 1;
    fn decode(bytes: &[u8]) -> Self {
        bytes.first().copied().unwrap_or_default()
    }
    fn encode(&self, out: &mut [u8]) {
        out[0] = *self;
    }
}

impl Value for Duration {
    const SIZE: usize = 8;
    fn decode(bytes: &[u8]) -> Self {
        let ns = bytes.try_into().map(i64::from_le_bytes).unwrap_or_default();
        Self::from_nanos(u64::try_from(ns).unwrap_or(0))
    }
    fn encode(&self, out: &mut [u8]) {
        let ns = i64::try_from(self.as_nanos()).unwrap_or(i64::MAX);
        out.copy_from_slice(&ns.to_le_bytes());
    }
}

macro_rules! enum_value {
    ($($name:ident),*) => {$(
        impl Value for $name {
            const SIZE: usize = 1;
            fn decode(bytes: &[u8]) -> Self {
                Self::from_value(bytes.first().copied().unwrap_or_default())
            }
            fn encode(&self, out: &mut [u8]) {
                out[0] = self.value();
            }
        }
    )*};
}

enum_value!(
    TombstoneMode,
    WeatherFrequency,
    Difficulty,
    Amount,
    StartleRepercussion,
    CurseModifier
);

macro_rules! settings {
    ($($(#[$doc:meta])* $field:ident: $ty:ty = $reflected:literal,)*) => {
        /// The server's game settings, typed; read and changed through
        /// [`GameSettings`]. Factors are 1 for the game's normal value.
        ///
        /// # Examples
        ///
        /// See the [module](self).
        #[derive(Clone, Copy, Debug, PartialEq)]
        #[non_exhaustive]
        pub struct Settings {
            $($(#[$doc])* pub $field: $ty,)*
        }

        /// Where each field lies, from reflection.
        #[derive(Clone, Copy, Debug)]
        struct Offsets {
            $($field: usize,)*
        }

        impl Offsets {
            fn resolve(load: &Load<'_>, size: usize) -> Result<Self, Off> {
                Ok(Self {
                    $($field: {
                        let at = usize::try_from(load.field_offset(TYPE, $reflected)?)
                            .map_err(|_| Off::new(concat!("game settings: ", $reflected)))?;
                        if at + <$ty as Value>::SIZE > size {
                            return Err(Off::new(concat!("game settings: ", $reflected, " past the end")));
                        }
                        at
                    },)*
                })
            }

            fn decode(&self, bytes: &[u8]) -> Settings {
                Settings {
                    $($field: <$ty as Value>::decode(&bytes[self.$field..self.$field + <$ty as Value>::SIZE]),)*
                }
            }

            fn encode(&self, settings: &Settings, bytes: &mut [u8]) {
                $(settings.$field.encode(&mut bytes[self.$field..self.$field + <$ty as Value>::SIZE]);)*
            }
        }

        impl Settings {
            fn factors(&self) -> impl Iterator<Item = f32> {
                [$(self.$field.factor(),)*].into_iter().flatten()
            }
        }
    };
}

settings! {
    /// The characters' health.
    player_health: f32 = "playerHealthFactor",
    /// The characters' mana.
    player_mana: f32 = "playerManaFactor",
    /// The characters' stamina.
    player_stamina: f32 = "playerStaminaFactor",
    /// The characters' body heat.
    player_body_heat: f32 = "playerBodyHeatFactor",
    /// How long characters hold their breath.
    player_diving_time: f32 = "playerDivingTimeFactor",
    /// Whether items wear out.
    durability: bool = "enableDurability",
    /// Whether hunger turns into starving.
    starving: bool = "enableStarvingDebuff",
    /// How long food buffs last.
    food_buff_duration: f32 = "foodBuffDurationFactor",
    /// How long a character goes hungry before starving, in game time.
    hunger_to_starving: Duration = "fromHungerToStarving",
    /// How long characters last in the shroud.
    shroud_time: f32 = "shroudTimeFactor",
    /// What a dead character leaves behind.
    tombstone: TombstoneMode = "tombstoneMode",
    /// Whether gliders meet turbulence.
    glider_turbulence: bool = "enableGliderTurbulences",
    /// How often rain and storms come.
    weather: WeatherFrequency = "weatherFrequency",
    /// How hard fishing is.
    fishing: Difficulty = "fishingDifficulty",
    /// Damage to terrain and resources when mining.
    mining_damage: f32 = "miningDamageFactor",
    /// How fast plants grow.
    plant_growth: f32 = "plantGrowthSpeedFactor",
    /// How many resources a drop stacks.
    resource_drops: f32 = "resourceDropStackAmountFactor",
    /// How fast workshops produce.
    factory_speed: f32 = "factoryProductionSpeedFactor",
    /// What recycling a perk upgrade returns.
    perk_recycling: f32 = "perkUpgradeRecyclingFactor",
    /// What perks cost.
    perk_cost: f32 = "perkCostFactor",
    /// Experience from combat.
    experience_combat: f32 = "experienceCombatFactor",
    /// Experience from mining.
    experience_mining: f32 = "experienceMiningFactor",
    /// Experience from exploration and quests.
    experience_exploration: f32 = "experienceExplorationQuestsFactor",
    /// How many enemies random spawners bring.
    random_spawns: Amount = "randomSpawnerAmount",
    /// How many enemies attack at once.
    aggro_pool: Amount = "aggroPoolAmount",
    /// The enemies' damage, for enemies spawned after a change.
    enemy_damage: f32 = "enemyDamageFactor",
    /// The enemies' health, for enemies spawned after a change.
    enemy_health: f32 = "enemyHealthFactor",
    /// The enemies' stamina, for enemies spawned after a change.
    enemy_stamina: f32 = "enemyStaminaFactor",
    /// How far enemies notice characters.
    enemy_perception: f32 = "enemyPerceptionRangeFactor",
    /// The bosses' damage.
    boss_damage: f32 = "bossDamageFactor",
    /// The bosses' health.
    boss_health: f32 = "bossHealthFactor",
    /// The threat bonus.
    threat_bonus: f32 = "threatBonus",
    /// Whether enemies leave characters alone until attacked.
    pacify_enemies: bool = "pacifyAllEnemies",
    /// What startling a tamed animal costs.
    startle: StartleRepercussion = "tamingStartleRepercussion",
    /// How long the day lasts in real time; the clock keeps its hour.
    day_length: Duration = "dayTimeDuration",
    /// How long the night lasts in real time.
    night_length: Duration = "nightTimeDuration",
    /// How hard the shroud's curse hits.
    curse: CurseModifier = "curseModifier",
    /// The Flame level from which the shroud lifts where it is cleared.
    flame_level_fog_removal: u8 = "flameLevelFogRemoval",
}

impl Settings {
    /// Whether the game can run with these values: factors finite and not
    /// negative, day and night lengths above zero.
    fn valid(&self) -> bool {
        self.factors().all(|f| f.is_finite() && f >= 0.0)
            && !self.day_length.is_zero()
            && !self.night_length.is_zero()
    }
}

/// The server's game settings; get it from [`Load::game_settings`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct GameSettings {
    host: Host,
    offsets: Offsets,
    size: usize,
}

impl GameSettings {
    /// The settings in force and the preset they came from; `None` off the
    /// game thread or while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn durability(settings: &enshwick_loader_api::game_settings::GameSettings) -> Option<bool> {
    ///     settings.read().map(|(now, _preset)| now.durability)
    /// }
    /// ```
    #[must_use]
    pub fn read(&self) -> Option<(Settings, Preset)> {
        let (bytes, preset) = self.raw().ok()?;
        Some((self.offsets.decode(&bytes), preset))
    }

    /// Lets `change` edit the settings in force and applies them as the
    /// game's admin command does: systems see them from the next step and
    /// every client gets them; the preset becomes [`Preset::Custom`].
    /// Returns the settings now in force.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a negative or not finite factor or a day
    /// or night length of zero, [`WorldError::WrongThread`] off the game
    /// thread and [`WorldError::Missing`] while no world runs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn change(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, WorldError> {
        let (bytes, settings) = self.changed(change)?;
        let size = u32::try_from(bytes.len()).map_err(|_| WorldError::Missing)?;
        let status = (self.host.api().set_game_settings)(self.host.handle(), bytes.as_ptr(), size);
        WorldError::from_status(status).map(|()| settings)
    }

    /// Changes the settings as [`change`](Self::change) does and keeps the
    /// fields whose value it changed with the running world: the loader
    /// applies them again each time the world loads, by the end of its
    /// first step, over what the configuration says. Fields it leaves as
    /// they were are not kept.
    /// Several mods' kept fields add up; the last value kept for a field
    /// wins.
    ///
    /// # Errors
    ///
    /// As for [`change`](Self::change).
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let settings = load.game_settings()?;
    ///     load.add_command("no-tombstone", "keeps everything on death, in this world", move |_ctx, _args| {
    ///         use enshwick_loader_api::game_settings::TombstoneMode;
    ///         match settings.keep(|s| s.tombstone = TombstoneMode::NoTombstone) {
    ///             Ok(_) => "kept for this world".into(),
    ///             Err(why) => format!("unchanged: {why}").into(),
    ///         }
    ///     })
    /// }
    /// ```
    pub fn keep(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, WorldError> {
        let (before, _preset) = self.raw()?;
        let (bytes, settings) = self.changed(change)?;
        let mask: Vec<u8> = before
            .iter()
            .zip(&bytes)
            .map(|(a, b)| if a == b { 0 } else { 0xff })
            .collect();
        let size = u32::try_from(bytes.len()).map_err(|_| WorldError::Missing)?;
        let status = (self.host.api().keep_game_settings)(
            self.host.handle(),
            bytes.as_ptr(),
            mask.as_ptr(),
            size,
        );
        WorldError::from_status(status).map(|()| settings)
    }

    /// Forgets every field kept for the running world, by any mod, and puts
    /// back the values the world loaded with.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the game thread,
    /// [`WorldError::Missing`] while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn reset(settings: &enshwick_loader_api::game_settings::GameSettings) {
    ///     let _forgotten = settings.forget_kept();
    /// }
    /// ```
    pub fn forget_kept(&self) -> Result<(), WorldError> {
        WorldError::from_status((self.host.api().forget_game_settings)(self.host.handle()))
    }

    /// The bytes of the settings in force after `change`, and the settings.
    fn changed(
        &self,
        change: impl FnOnce(&mut Settings),
    ) -> Result<(Vec<u8>, Settings), WorldError> {
        let (mut bytes, _preset) = self.raw()?;
        let mut settings = self.offsets.decode(&bytes);
        change(&mut settings);
        if !settings.valid() {
            return Err(WorldError::Refused);
        }
        self.offsets.encode(&settings, &mut bytes);
        Ok((bytes, settings))
    }

    fn raw(&self) -> Result<(Vec<u8>, Preset), WorldError> {
        let mut bytes = vec![0u8; self.size];
        let size = u32::try_from(self.size).map_err(|_| WorldError::Missing)?;
        let mut preset = 0u32;
        let status = (self.host.api().game_settings)(
            self.host.handle(),
            bytes.as_mut_ptr(),
            size,
            &raw mut preset,
        );
        WorldError::from_status(status)?;
        Ok((
            bytes,
            Preset::from_value(u8::try_from(preset).unwrap_or(u8::MAX)),
        ))
    }
}

impl Load<'_> {
    /// The server's [`GameSettings`]; on a client, the built-in server's
    /// while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build whose settings lack a field this API knows.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn game_settings(&mut self) -> Result<GameSettings, Off> {
        let host = self.host();
        let size =
            usize::try_from(self.type_size(TYPE)?).map_err(|_| Off::new("game settings: size"))?;
        Ok(GameSettings {
            host,
            offsets: Offsets::resolve(self, size)?,
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_keep_values_they_do_not_name() {
        assert_eq!(WeatherFrequency::from_value(3), WeatherFrequency::Often);
        assert_eq!(WeatherFrequency::from_value(9), WeatherFrequency::Other(9));
        assert_eq!(WeatherFrequency::Other(9).value(), 9);
        assert_eq!(Amount::Automatic.value(), 4);
    }

    #[test]
    fn durations_round_trip_through_nanoseconds() {
        let mut bytes = [0u8; 8];
        Duration::from_mins(30).encode(&mut bytes);
        assert_eq!(i64::from_le_bytes(bytes), 1_800_000_000_000);
        assert_eq!(Duration::decode(&bytes), Duration::from_mins(30));
    }
}

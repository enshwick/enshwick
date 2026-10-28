//! The server's game settings: the difficulty and gameplay values of its
//! configuration (`gameSettings` in `enshrouded_server.json`) or of the
//! host's world menu, read typed and changed while the world runs.
//!
//! Every system reads the live settings when it runs, so a change takes
//! effect from the next step and every client gets it. Some values act on
//! what comes next: enemy health and damage on enemies spawned after the
//! change, food buffs on food eaten after it, the tombstone mode on the next
//! death. A [`change`](GameSettings::change) lasts until the server stops,
//! unless the server saves its configuration first: a dedicated server
//! writes its game settings back to its configuration file after a ban, an
//! unban or an admin's settings change.
//! [`keep`](GameSettings::keep) changes them the same way and keeps the
//! fields it changed with the world: the loader applies them again each time
//! that world loads, over what the configuration says, until
//! [`forget_kept`](GameSettings::forget_kept).
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::game_settings::GameSettings;
//!
//! fn harder(settings: &GameSettings) -> Result<f32, enshwick::WorldError> {
//!     Ok(settings.change(|s| s.enemy_damage_factor *= 2.0)?.enemy_damage_factor)
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<GameSettings, enshwick::Off> {
//!     load.game_settings()
//! }
//! ```

use core::time::Duration;

use crate::WorldError;
use crate::content::EngineFacts;
use crate::engine_enum::Values;
use crate::engine_ids as ids;
use crate::{Host, Load, Off};

macro_rules! game_enum {
    ($(#[$doc:meta])* $name:ident: $field:ident { $($(#[$vdoc:meta])* $variant:ident = $id:path,)* }) => {
        $(#[$doc])*
        /// The loader reads the game's value of each variant by its
        /// constant's name.
        ///
        /// # Examples
        ///
        /// See [`Settings`].
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vdoc])* $variant,)*
            /// A value this API does not name, kept as the game gave it.
            Other(u8),
        }

        impl $name {
            const COUNT: usize = [$(stringify!($variant)),*].len();

            /// Each variant with the id of the game's constant for it.
            const IDS: [(Self, &'static str); Self::COUNT] = [$((Self::$variant, $id)),*];
        }

        impl Value for $name {
            const SIZE: usize = 1;
            fn decode(bytes: &[u8], enums: &Enums) -> Self {
                let raw = bytes.first().copied().unwrap_or_default();
                enums.$field.variant(u64::from(raw)).unwrap_or(Self::Other(raw))
            }
            fn encode(&self, out: &mut [u8], enums: &Enums) -> bool {
                let value = match *self {
                    Self::Other(raw) => Some(raw),
                    variant => enums.$field.value(variant).and_then(|v| u8::try_from(v).ok()),
                };
                value.map(|value| out[0] = value).is_some()
            }
        }
    };
}

game_enum! {
    /// The preset the server's settings came from.
    GameSettingsPreset: preset {
        /// The game's defaults.
        Default = ids::GAME_SETTINGS_PRESET_DEFAULT,
        /// Easier.
        Relaxed = ids::GAME_SETTINGS_PRESET_RELAXED,
        /// Harder.
        Hard = ids::GAME_SETTINGS_PRESET_HARD,
        /// Hardest, with hunger.
        Survival = ids::GAME_SETTINGS_PRESET_SURVIVAL,
        /// Values of their own; what every change makes the preset.
        Custom = ids::GAME_SETTINGS_PRESET_CUSTOM,
    }
}

game_enum! {
    /// What a dead character leaves behind.
    TombstoneMode: tombstone_mode {
        /// The backpack's materials go into the tombstone.
        AddBackpackMaterials = ids::TOMBSTONE_MODE_ADD_BACKPACK_MATERIALS,
        /// Everything the character carried goes into it.
        Everything = ids::TOMBSTONE_MODE_EVERYTHING,
        /// No tombstone: the character keeps everything.
        NoTombstone = ids::TOMBSTONE_MODE_NO_TOMBSTONE,
    }
}

game_enum! {
    /// How often rain and storms come.
    WeatherFrequency: weather_frequency {
        /// Never.
        Disabled = ids::WEATHER_FREQUENCY_DISABLED,
        /// Half as often.
        Rare = ids::WEATHER_FREQUENCY_RARE,
        /// As the game means it.
        Normal = ids::WEATHER_FREQUENCY_NORMAL,
        /// Twice as often.
        Often = ids::WEATHER_FREQUENCY_OFTEN,
    }
}

game_enum! {
    /// A difficulty from very easy to very hard, such as fishing's.
    GenericDifficulty: generic_difficulty {
        /// Very easy.
        VeryEasy = ids::GENERIC_DIFFICULTY_VERY_EASY,
        /// Easy.
        Easy = ids::GENERIC_DIFFICULTY_EASY,
        /// Normal.
        Normal = ids::GENERIC_DIFFICULTY_NORMAL,
        /// Hard.
        Hard = ids::GENERIC_DIFFICULTY_HARD,
        /// Very hard.
        VeryHard = ids::GENERIC_DIFFICULTY_VERY_HARD,
    }
}

game_enum! {
    /// How many enemies random spawners bring.
    RandomSpawnerAmount: random_spawner_amount {
        /// Few.
        Few = ids::RANDOM_SPAWNER_AMOUNT_FEW,
        /// Normal.
        Normal = ids::RANDOM_SPAWNER_AMOUNT_NORMAL,
        /// Many.
        Many = ids::RANDOM_SPAWNER_AMOUNT_MANY,
        /// Extreme.
        Extreme = ids::RANDOM_SPAWNER_AMOUNT_EXTREME,
        /// By the number of players.
        Automatic = ids::RANDOM_SPAWNER_AMOUNT_AUTOMATIC,
    }
}

game_enum! {
    /// How many enemies attack at once.
    AggroPoolAmount: aggro_pool_amount {
        /// Few.
        Few = ids::AGGRO_POOL_AMOUNT_FEW,
        /// Normal.
        Normal = ids::AGGRO_POOL_AMOUNT_NORMAL,
        /// Many.
        Many = ids::AGGRO_POOL_AMOUNT_MANY,
        /// Extreme.
        Extreme = ids::AGGRO_POOL_AMOUNT_EXTREME,
    }
}

game_enum! {
    /// What startling a tamed animal costs its taming.
    TamingStartleRepercussion: taming_startle_repercussion {
        /// Nothing.
        KeepProgress = ids::TAMING_STARTLE_REPERCUSSION_KEEP_PROGRESS,
        /// Some of the progress.
        LoseSomeProgress = ids::TAMING_STARTLE_REPERCUSSION_LOSE_SOME_PROGRESS,
        /// All of it.
        LoseAllProgress = ids::TAMING_STARTLE_REPERCUSSION_LOSE_ALL_PROGRESS,
    }
}

game_enum! {
    /// How hard the shroud's curse hits.
    CurseModifier: curse_modifier {
        /// Easy.
        Easy = ids::CURSE_MODIFIER_EASY,
        /// Normal.
        Normal = ids::CURSE_MODIFIER_NORMAL,
        /// Hard.
        Hard = ids::CURSE_MODIFIER_HARD,
    }
}

/// The game's value of each variant of the settings' enums, for the
/// running build.
#[derive(Clone, Copy, Debug)]
struct Enums {
    preset: Values<GameSettingsPreset, { GameSettingsPreset::COUNT }>,
    tombstone_mode: Values<TombstoneMode, { TombstoneMode::COUNT }>,
    weather_frequency: Values<WeatherFrequency, { WeatherFrequency::COUNT }>,
    generic_difficulty: Values<GenericDifficulty, { GenericDifficulty::COUNT }>,
    random_spawner_amount: Values<RandomSpawnerAmount, { RandomSpawnerAmount::COUNT }>,
    aggro_pool_amount: Values<AggroPoolAmount, { AggroPoolAmount::COUNT }>,
    taming_startle_repercussion:
        Values<TamingStartleRepercussion, { TamingStartleRepercussion::COUNT }>,
    curse_modifier: Values<CurseModifier, { CurseModifier::COUNT }>,
}

impl Enums {
    /// Tables without a value for any variant.
    #[cfg(test)]
    fn none() -> Self {
        Self {
            preset: Values::new(GameSettingsPreset::IDS.map(|(v, _)| (v, None))),
            tombstone_mode: Values::new(TombstoneMode::IDS.map(|(v, _)| (v, None))),
            weather_frequency: Values::new(WeatherFrequency::IDS.map(|(v, _)| (v, None))),
            generic_difficulty: Values::new(GenericDifficulty::IDS.map(|(v, _)| (v, None))),
            random_spawner_amount: Values::new(RandomSpawnerAmount::IDS.map(|(v, _)| (v, None))),
            aggro_pool_amount: Values::new(AggroPoolAmount::IDS.map(|(v, _)| (v, None))),
            taming_startle_repercussion: Values::new(
                TamingStartleRepercussion::IDS.map(|(v, _)| (v, None)),
            ),
            curse_modifier: Values::new(CurseModifier::IDS.map(|(v, _)| (v, None))),
        }
    }

    fn resolve(load: &Load<'_>) -> Self {
        Self {
            preset: Values::resolve(load, GameSettingsPreset::IDS),
            tombstone_mode: Values::resolve(load, TombstoneMode::IDS),
            weather_frequency: Values::resolve(load, WeatherFrequency::IDS),
            generic_difficulty: Values::resolve(load, GenericDifficulty::IDS),
            random_spawner_amount: Values::resolve(load, RandomSpawnerAmount::IDS),
            aggro_pool_amount: Values::resolve(load, AggroPoolAmount::IDS),
            taming_startle_repercussion: Values::resolve(load, TamingStartleRepercussion::IDS),
            curse_modifier: Values::resolve(load, CurseModifier::IDS),
        }
    }
}

/// How a field is stored.
trait Value: Sized {
    const SIZE: usize;
    fn decode(bytes: &[u8], enums: &Enums) -> Self;
    /// Writes the value; `false` when the build has no value for it.
    fn encode(&self, out: &mut [u8], enums: &Enums) -> bool;
    /// The value, when it is a factor.
    fn factor(&self) -> Option<f32> {
        None
    }
}

impl Value for f32 {
    const SIZE: usize = 4;
    fn decode(bytes: &[u8], _: &Enums) -> Self {
        bytes
            .try_into()
            .map(Self::from_le_bytes)
            .unwrap_or_default()
    }
    fn encode(&self, out: &mut [u8], _: &Enums) -> bool {
        out.copy_from_slice(&self.to_le_bytes());
        true
    }
    fn factor(&self) -> Option<f32> {
        Some(*self)
    }
}

impl Value for bool {
    const SIZE: usize = 1;
    fn decode(bytes: &[u8], _: &Enums) -> Self {
        bytes.first().is_some_and(|&b| b != 0)
    }
    fn encode(&self, out: &mut [u8], _: &Enums) -> bool {
        out[0] = u8::from(*self);
        true
    }
}

impl Value for u8 {
    const SIZE: usize = 1;
    fn decode(bytes: &[u8], _: &Enums) -> Self {
        bytes.first().copied().unwrap_or_default()
    }
    fn encode(&self, out: &mut [u8], _: &Enums) -> bool {
        out[0] = *self;
        true
    }
}

impl Value for Duration {
    const SIZE: usize = 8;
    fn decode(bytes: &[u8], _: &Enums) -> Self {
        let ns = bytes.try_into().map(i64::from_le_bytes).unwrap_or_default();
        Self::from_nanos(u64::try_from(ns).unwrap_or(0))
    }
    fn encode(&self, out: &mut [u8], _: &Enums) -> bool {
        let ns = i64::try_from(self.as_nanos()).unwrap_or(i64::MAX);
        out.copy_from_slice(&ns.to_le_bytes());
        true
    }
}

macro_rules! settings {
    ($($(#[$doc:meta])* $field:ident: $ty:ty = $id:path,)*) => {
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

        /// Where each field lies, as the loader answers its id.
        #[derive(Clone, Copy, Debug)]
        struct Offsets {
            $($field: usize,)*
        }

        impl Offsets {
            fn resolve(load: &Load<'_>, size: usize) -> Result<Self, Off> {
                Ok(Self {
                    $($field: {
                        let at = load.offset($id)?;
                        if at + <$ty as Value>::SIZE > size {
                            return Err(Off::new(format!("{}: past the end", $id)));
                        }
                        at
                    },)*
                })
            }

            fn decode(&self, bytes: &[u8], enums: &Enums) -> Settings {
                Settings {
                    $($field: <$ty as Value>::decode(&bytes[self.$field..self.$field + <$ty as Value>::SIZE], enums),)*
                }
            }

            /// Writes `settings` into `bytes`; `false` when a field holds a
            /// variant the build has no value for.
            fn encode(&self, settings: &Settings, bytes: &mut [u8], enums: &Enums) -> bool {
                true $(&& settings.$field.encode(&mut bytes[self.$field..self.$field + <$ty as Value>::SIZE], enums))*
            }
        }

        /// Each field's name with the id of the game's field it is.
        #[cfg(test)]
        const FIELD_IDS: &[(&str, &str)] = &[$((stringify!($field), $id)),*];

        impl Settings {
            fn factors(&self) -> impl Iterator<Item = f32> {
                [$(self.$field.factor(),)*].into_iter().flatten()
            }
        }
    };
}

settings! {
    /// The characters' health.
    player_health_factor: f32 = ids::GAME_SETTINGS_PLAYER_HEALTH_FACTOR,
    /// The characters' mana.
    player_mana_factor: f32 = ids::GAME_SETTINGS_PLAYER_MANA_FACTOR,
    /// The characters' stamina.
    player_stamina_factor: f32 = ids::GAME_SETTINGS_PLAYER_STAMINA_FACTOR,
    /// The characters' body heat.
    player_body_heat_factor: f32 = ids::GAME_SETTINGS_PLAYER_BODY_HEAT_FACTOR,
    /// How long characters hold their breath.
    player_diving_time_factor: f32 = ids::GAME_SETTINGS_PLAYER_DIVING_TIME_FACTOR,
    /// Whether items wear out.
    enable_durability: bool = ids::GAME_SETTINGS_ENABLE_DURABILITY,
    /// Whether hunger turns into starving.
    enable_starving_debuff: bool = ids::GAME_SETTINGS_ENABLE_STARVING_DEBUFF,
    /// How long food buffs last.
    food_buff_duration_factor: f32 = ids::GAME_SETTINGS_FOOD_BUFF_DURATION_FACTOR,
    /// How long a character goes hungry before starving, in game time.
    from_hunger_to_starving: Duration = ids::GAME_SETTINGS_FROM_HUNGER_TO_STARVING,
    /// How long characters last in the shroud.
    shroud_time_factor: f32 = ids::GAME_SETTINGS_SHROUD_TIME_FACTOR,
    /// What a dead character leaves behind.
    tombstone_mode: TombstoneMode = ids::GAME_SETTINGS_TOMBSTONE_MODE,
    /// Whether gliders meet turbulence.
    enable_glider_turbulences: bool = ids::GAME_SETTINGS_ENABLE_GLIDER_TURBULENCES,
    /// How often rain and storms come.
    weather_frequency: WeatherFrequency = ids::GAME_SETTINGS_WEATHER_FREQUENCY,
    /// How hard fishing is.
    fishing_difficulty: GenericDifficulty = ids::GAME_SETTINGS_FISHING_DIFFICULTY,
    /// Damage to terrain and resources when mining.
    mining_damage_factor: f32 = ids::GAME_SETTINGS_MINING_DAMAGE_FACTOR,
    /// How fast plants grow.
    plant_growth_speed_factor: f32 = ids::GAME_SETTINGS_PLANT_GROWTH_SPEED_FACTOR,
    /// How many resources a drop stacks.
    resource_drop_stack_amount_factor: f32 = ids::GAME_SETTINGS_RESOURCE_DROP_STACK_AMOUNT_FACTOR,
    /// How fast workshops produce.
    factory_production_speed_factor: f32 = ids::GAME_SETTINGS_FACTORY_PRODUCTION_SPEED_FACTOR,
    /// What recycling a perk upgrade returns.
    perk_upgrade_recycling_factor: f32 = ids::GAME_SETTINGS_PERK_UPGRADE_RECYCLING_FACTOR,
    /// What perks cost.
    perk_cost_factor: f32 = ids::GAME_SETTINGS_PERK_COST_FACTOR,
    /// Experience from combat.
    experience_combat_factor: f32 = ids::GAME_SETTINGS_EXPERIENCE_COMBAT_FACTOR,
    /// Experience from mining.
    experience_mining_factor: f32 = ids::GAME_SETTINGS_EXPERIENCE_MINING_FACTOR,
    /// Experience from exploration and quests.
    experience_exploration_quests_factor: f32 = ids::GAME_SETTINGS_EXPERIENCE_EXPLORATION_QUESTS_FACTOR,
    /// How many enemies random spawners bring.
    random_spawner_amount: RandomSpawnerAmount = ids::GAME_SETTINGS_RANDOM_SPAWNER_AMOUNT,
    /// How many enemies attack at once.
    aggro_pool_amount: AggroPoolAmount = ids::GAME_SETTINGS_AGGRO_POOL_AMOUNT,
    /// The enemies' damage, for enemies spawned after a change.
    enemy_damage_factor: f32 = ids::GAME_SETTINGS_ENEMY_DAMAGE_FACTOR,
    /// The enemies' health, for enemies spawned after a change.
    enemy_health_factor: f32 = ids::GAME_SETTINGS_ENEMY_HEALTH_FACTOR,
    /// The enemies' stamina, for enemies spawned after a change.
    enemy_stamina_factor: f32 = ids::GAME_SETTINGS_ENEMY_STAMINA_FACTOR,
    /// How far enemies notice characters.
    enemy_perception_range_factor: f32 = ids::GAME_SETTINGS_ENEMY_PERCEPTION_RANGE_FACTOR,
    /// The bosses' damage.
    boss_damage_factor: f32 = ids::GAME_SETTINGS_BOSS_DAMAGE_FACTOR,
    /// The bosses' health.
    boss_health_factor: f32 = ids::GAME_SETTINGS_BOSS_HEALTH_FACTOR,
    /// The threat bonus.
    threat_bonus: f32 = ids::GAME_SETTINGS_THREAT_BONUS,
    /// Whether enemies leave characters alone until attacked.
    pacify_all_enemies: bool = ids::GAME_SETTINGS_PACIFY_ALL_ENEMIES,
    /// What startling a tamed animal costs.
    taming_startle_repercussion: TamingStartleRepercussion = ids::GAME_SETTINGS_TAMING_STARTLE_REPERCUSSION,
    /// How long the day lasts in real time; the clock keeps its hour.
    day_time_duration: Duration = ids::GAME_SETTINGS_DAY_TIME_DURATION,
    /// How long the night lasts in real time.
    night_time_duration: Duration = ids::GAME_SETTINGS_NIGHT_TIME_DURATION,
    /// How hard the shroud's curse hits.
    curse_modifier: CurseModifier = ids::GAME_SETTINGS_CURSE_MODIFIER,
    /// The Flame level from which the shroud lifts where it is cleared.
    flame_level_fog_removal: u8 = ids::GAME_SETTINGS_FLAME_LEVEL_FOG_REMOVAL,
}

impl Settings {
    /// Whether the game can run with these values: factors finite and not
    /// negative, day and night lengths above zero.
    fn valid(&self) -> bool {
        self.factors().all(|f| f.is_finite() && f >= 0.0)
            && !self.day_time_duration.is_zero()
            && !self.night_time_duration.is_zero()
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
    enums: Enums,
    size: usize,
}

impl GameSettings {
    /// The settings in force and the preset they came from; `None` off the
    /// game thread or while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn durability(settings: &enshwick::game_settings::GameSettings) -> Option<bool> {
    ///     settings.read().map(|(now, _preset)| now.enable_durability)
    /// }
    /// ```
    #[must_use]
    pub fn read(&self) -> Option<(Settings, GameSettingsPreset)> {
        let (bytes, preset) = self.raw().ok()?;
        Some((self.offsets.decode(&bytes, &self.enums), preset))
    }

    /// Lets `change` edit the settings in force and applies them as the
    /// game's admin command does: systems see them from the next step and
    /// every client gets them; the preset becomes [`GameSettingsPreset::Custom`].
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
    /// use enshwick::game_settings::{GameSettings, TombstoneMode};
    ///
    /// fn keep_everything(settings: &GameSettings) -> Result<(), enshwick::WorldError> {
    ///     settings.keep(|s| s.tombstone_mode = TombstoneMode::NoTombstone)?;
    ///     Ok(())
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
    /// fn reset(settings: &enshwick::game_settings::GameSettings) {
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
        let mut settings = self.offsets.decode(&bytes, &self.enums);
        change(&mut settings);
        if !settings.valid() || !self.offsets.encode(&settings, &mut bytes, &self.enums) {
            return Err(WorldError::Refused);
        }
        Ok((bytes, settings))
    }

    fn raw(&self) -> Result<(Vec<u8>, GameSettingsPreset), WorldError> {
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
        let preset = u8::try_from(preset).map_or(GameSettingsPreset::Other(u8::MAX), |raw| {
            self.enums
                .preset
                .variant(u64::from(raw))
                .unwrap_or(GameSettingsPreset::Other(raw))
        });
        Ok((bytes, preset))
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
        let size = self.size(ids::GAME_SETTINGS)?;
        Ok(GameSettings {
            host,
            offsets: Offsets::resolve(self, size)?,
            enums: Enums::resolve(self),
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_and_variant_takes_the_game_s_name() {
        for (field, id) in FIELD_IDS {
            assert_eq!(Some(*field), id.strip_prefix("game_settings."), "{id}");
        }
        crate::engine_enum::assert_named_after_engine(&GameSettingsPreset::IDS);
        crate::engine_enum::assert_named_after_engine(&TombstoneMode::IDS);
        crate::engine_enum::assert_named_after_engine(&WeatherFrequency::IDS);
        crate::engine_enum::assert_named_after_engine(&GenericDifficulty::IDS);
        crate::engine_enum::assert_named_after_engine(&RandomSpawnerAmount::IDS);
        crate::engine_enum::assert_named_after_engine(&AggroPoolAmount::IDS);
        crate::engine_enum::assert_named_after_engine(&TamingStartleRepercussion::IDS);
        crate::engine_enum::assert_named_after_engine(&CurseModifier::IDS);
    }

    #[test]
    fn enums_read_the_game_s_values_and_keep_those_they_do_not_name() {
        let mut enums = Enums::none();
        enums.weather_frequency = Values::new([
            (WeatherFrequency::Disabled, Some(7)),
            (WeatherFrequency::Rare, Some(8)),
            (WeatherFrequency::Normal, None),
            (WeatherFrequency::Often, Some(5)),
        ]);
        assert_eq!(
            WeatherFrequency::decode(&[5], &enums),
            WeatherFrequency::Often
        );
        assert_eq!(
            WeatherFrequency::decode(&[9], &enums),
            WeatherFrequency::Other(9)
        );
        let mut out = [0u8];
        assert!(WeatherFrequency::Disabled.encode(&mut out, &enums));
        assert_eq!(out, [7]);
        assert!(WeatherFrequency::Other(9).encode(&mut out, &enums));
        assert_eq!(out, [9]);
        assert!(
            !WeatherFrequency::Normal.encode(&mut out, &enums),
            "a variant the build has no value for"
        );
    }

    #[test]
    fn durations_round_trip_through_nanoseconds() {
        let mut bytes = [0u8; 8];
        let enums = Enums::none();
        assert!(Duration::from_mins(30).encode(&mut bytes, &enums));
        assert_eq!(i64::from_le_bytes(bytes), 1_800_000_000_000);
        assert_eq!(Duration::decode(&bytes, &enums), Duration::from_mins(30));
    }
}

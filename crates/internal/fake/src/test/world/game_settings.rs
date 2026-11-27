//! The server's game settings as the game lays them out, and the values of
//! its Default preset, so a mod's test reads them without the engine's ids.

use enshwick_core::__fake::ids;

use crate::test::{FakeHost, Installed};

/// The settings struct's size, as server 26430253 reflects it.
const SIZE: u16 = 144;

/// A field's value in the game's Default preset.
#[derive(Clone, Copy)]
enum Value {
    F32(f32),
    Bool(bool),
    U8(u8),
    /// An enum's variant, by the id of its constant in [`ENUM_VALUES`].
    Variant(&'static str),
    /// A length of time, in nanoseconds.
    Nanos(i64),
}

/// Each field with its offset, as server 26430253 reflects it, and its value
/// in the dedicated server's Default preset (`enshrouded_server.json`).
const FIELDS: [(&str, u16, Value); 38] = [
    (
        ids::GAME_SETTINGS_PLAYER_HEALTH_FACTOR,
        0x00,
        Value::F32(1.0),
    ),
    (ids::GAME_SETTINGS_PLAYER_MANA_FACTOR, 0x04, Value::F32(1.0)),
    (
        ids::GAME_SETTINGS_PLAYER_STAMINA_FACTOR,
        0x08,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_PLAYER_BODY_HEAT_FACTOR,
        0x0c,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_PLAYER_DIVING_TIME_FACTOR,
        0x10,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_ENABLE_DURABILITY,
        0x14,
        Value::Bool(true),
    ),
    (
        ids::GAME_SETTINGS_ENABLE_STARVING_DEBUFF,
        0x15,
        Value::Bool(false),
    ),
    (
        ids::GAME_SETTINGS_FOOD_BUFF_DURATION_FACTOR,
        0x18,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_FROM_HUNGER_TO_STARVING,
        0x20,
        Value::Nanos(600_000_000_000),
    ),
    (ids::GAME_SETTINGS_SHROUD_TIME_FACTOR, 0x28, Value::F32(1.0)),
    (
        ids::GAME_SETTINGS_TOMBSTONE_MODE,
        0x2c,
        Value::Variant(ids::TOMBSTONE_MODE_ADD_BACKPACK_MATERIALS),
    ),
    (
        ids::GAME_SETTINGS_ENABLE_GLIDER_TURBULENCES,
        0x2d,
        Value::Bool(true),
    ),
    (
        ids::GAME_SETTINGS_WEATHER_FREQUENCY,
        0x2e,
        Value::Variant(ids::WEATHER_FREQUENCY_NORMAL),
    ),
    (
        ids::GAME_SETTINGS_FISHING_DIFFICULTY,
        0x2f,
        Value::Variant(ids::GENERIC_DIFFICULTY_NORMAL),
    ),
    (
        ids::GAME_SETTINGS_MINING_DAMAGE_FACTOR,
        0x30,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_PLANT_GROWTH_SPEED_FACTOR,
        0x34,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_RESOURCE_DROP_STACK_AMOUNT_FACTOR,
        0x38,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_FACTORY_PRODUCTION_SPEED_FACTOR,
        0x3c,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_PERK_UPGRADE_RECYCLING_FACTOR,
        0x40,
        Value::F32(0.5),
    ),
    (ids::GAME_SETTINGS_PERK_COST_FACTOR, 0x44, Value::F32(1.0)),
    (
        ids::GAME_SETTINGS_EXPERIENCE_COMBAT_FACTOR,
        0x48,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_EXPERIENCE_MINING_FACTOR,
        0x4c,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_EXPERIENCE_EXPLORATION_QUESTS_FACTOR,
        0x50,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_RANDOM_SPAWNER_AMOUNT,
        0x54,
        Value::Variant(ids::RANDOM_SPAWNER_AMOUNT_AUTOMATIC),
    ),
    (
        ids::GAME_SETTINGS_AGGRO_POOL_AMOUNT,
        0x55,
        Value::Variant(ids::AGGRO_POOL_AMOUNT_NORMAL),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_DAMAGE_FACTOR,
        0x58,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_HEALTH_FACTOR,
        0x5c,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_STAMINA_FACTOR,
        0x60,
        Value::F32(1.0),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_PERCEPTION_RANGE_FACTOR,
        0x64,
        Value::F32(1.0),
    ),
    (ids::GAME_SETTINGS_BOSS_DAMAGE_FACTOR, 0x68, Value::F32(1.0)),
    (ids::GAME_SETTINGS_BOSS_HEALTH_FACTOR, 0x6c, Value::F32(1.0)),
    (ids::GAME_SETTINGS_THREAT_BONUS, 0x70, Value::F32(1.0)),
    (
        ids::GAME_SETTINGS_PACIFY_ALL_ENEMIES,
        0x74,
        Value::Bool(false),
    ),
    (
        ids::GAME_SETTINGS_TAMING_STARTLE_REPERCUSSION,
        0x75,
        Value::Variant(ids::TAMING_STARTLE_REPERCUSSION_LOSE_SOME_PROGRESS),
    ),
    (
        ids::GAME_SETTINGS_DAY_TIME_DURATION,
        0x78,
        Value::Nanos(1_800_000_000_000),
    ),
    (
        ids::GAME_SETTINGS_NIGHT_TIME_DURATION,
        0x80,
        Value::Nanos(720_000_000_000),
    ),
    (
        ids::GAME_SETTINGS_CURSE_MODIFIER,
        0x88,
        Value::Variant(ids::CURSE_MODIFIER_NORMAL),
    ),
    (
        ids::GAME_SETTINGS_FLAME_LEVEL_FOG_REMOVAL,
        0x89,
        Value::U8(11),
    ),
];

/// The game's value of every variant of the settings' enums, as server
/// 26430253 reflects them.
const ENUM_VALUES: [(&str, u8); 32] = [
    (ids::GAME_SETTINGS_PRESET_DEFAULT, 0),
    (ids::GAME_SETTINGS_PRESET_RELAXED, 1),
    (ids::GAME_SETTINGS_PRESET_HARD, 2),
    (ids::GAME_SETTINGS_PRESET_SURVIVAL, 3),
    (ids::GAME_SETTINGS_PRESET_CUSTOM, 4),
    (ids::TOMBSTONE_MODE_ADD_BACKPACK_MATERIALS, 0),
    (ids::TOMBSTONE_MODE_EVERYTHING, 1),
    (ids::TOMBSTONE_MODE_NO_TOMBSTONE, 2),
    (ids::WEATHER_FREQUENCY_DISABLED, 0),
    (ids::WEATHER_FREQUENCY_RARE, 1),
    (ids::WEATHER_FREQUENCY_NORMAL, 2),
    (ids::WEATHER_FREQUENCY_OFTEN, 3),
    (ids::GENERIC_DIFFICULTY_VERY_EASY, 0),
    (ids::GENERIC_DIFFICULTY_EASY, 1),
    (ids::GENERIC_DIFFICULTY_NORMAL, 2),
    (ids::GENERIC_DIFFICULTY_HARD, 3),
    (ids::GENERIC_DIFFICULTY_VERY_HARD, 4),
    (ids::RANDOM_SPAWNER_AMOUNT_FEW, 0),
    (ids::RANDOM_SPAWNER_AMOUNT_NORMAL, 1),
    (ids::RANDOM_SPAWNER_AMOUNT_MANY, 2),
    (ids::RANDOM_SPAWNER_AMOUNT_EXTREME, 3),
    (ids::RANDOM_SPAWNER_AMOUNT_AUTOMATIC, 4),
    (ids::AGGRO_POOL_AMOUNT_FEW, 0),
    (ids::AGGRO_POOL_AMOUNT_NORMAL, 1),
    (ids::AGGRO_POOL_AMOUNT_MANY, 2),
    (ids::AGGRO_POOL_AMOUNT_EXTREME, 3),
    (ids::TAMING_STARTLE_REPERCUSSION_KEEP_PROGRESS, 0),
    (ids::TAMING_STARTLE_REPERCUSSION_LOSE_SOME_PROGRESS, 1),
    (ids::TAMING_STARTLE_REPERCUSSION_LOSE_ALL_PROGRESS, 2),
    (ids::CURSE_MODIFIER_EASY, 0),
    (ids::CURSE_MODIFIER_NORMAL, 1),
    (ids::CURSE_MODIFIER_HARD, 2),
];

/// The game's value of the enum constant `id`; 0 for one [`ENUM_VALUES`]
/// lacks, which its test rules out.
fn value_of(id: &str) -> u8 {
    ENUM_VALUES
        .iter()
        .find(|&&(named, _)| named == id)
        .map_or(0, |&(_, value)| value)
}

/// The Default preset's settings as bytes of the game's layout.
fn defaults() -> Vec<u8> {
    let mut bytes = vec![0; usize::from(SIZE)];
    for (_, at, value) in FIELDS {
        let at = usize::from(at);
        match value {
            Value::F32(factor) => bytes[at..at + 4].copy_from_slice(&factor.to_le_bytes()),
            Value::Bool(on) => bytes[at] = u8::from(on),
            Value::U8(raw) => bytes[at] = raw,
            Value::Variant(id) => bytes[at] = value_of(id),
            Value::Nanos(ns) => bytes[at..at + 8].copy_from_slice(&ns.to_le_bytes()),
        }
    }
    bytes
}

impl FakeHost {
    /// The game settings' size, the offset of every field and the value of
    /// every enum variant as the current build lays them out, so
    /// `Load::game_settings` resolves; until the test sets values with
    /// [`Installed::set_default_game_settings`] or
    /// [`Installed::set_game_settings`], a read answers
    /// `WorldError::Unavailable`, as while no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().game_settings().install();
    /// loader.set_default_game_settings();
    /// assert_eq!(loader.game_settings().map(|(_, preset)| preset), Some(0));
    /// ```
    #[must_use]
    pub fn game_settings(self) -> Self {
        let offsets = FIELDS.map(|(id, at, _)| (id, u64::from(at)));
        let values = ENUM_VALUES.map(|(id, value)| (id, u64::from(value)));
        self.preset(&[], &[(ids::GAME_SETTINGS, u32::from(SIZE))], &offsets)
            .preset(&[], &[], &values)
    }
}

impl Installed {
    /// Sets the game settings the fake reports to the dedicated server's
    /// Default preset, laid out as [`FakeHost::game_settings`] lays them:
    /// factors 1 (perk upgrade recycling 0.5), durability on, normal
    /// weather, a 30-minute day and a 12-minute night.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::game_settings`].
    pub fn set_default_game_settings(&self) {
        self.set_game_settings(
            defaults(),
            u32::from(value_of(ids::GAME_SETTINGS_PRESET_DEFAULT)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_variant_has_a_value_and_every_field_fits() {
        for (id, at, value) in FIELDS {
            if let Value::Variant(variant) = value {
                assert!(
                    ENUM_VALUES.iter().any(|&(named, _)| named == variant),
                    "{id}"
                );
            }
            assert!(at < SIZE, "{id}");
        }
    }
}

//! A copy of each survivor at every Flame Altar: a cluster of N altars holds at
//! most N copies of a survivor, each copy stands inside a zone, works there and
//! sleeps in a bed of its own altar.
//!
//! A summoning staff used in an altar zone without a copy places one there; in
//! a zone with a copy it moves that copy. The original survivor, the one the
//! game's own record names, is never touched.

mod client;
mod copies;
mod cursor;
mod server;
mod staffs;

use enshwick_api::abi::Side;
use enshwick_api::{Load, Mod, Off};

const DEFAULTS: &str = "\
# A copy of each survivor at every altar, at most one per altar zone.
per_altar = true
# Copy staffs: one per workshop survivor, used up when it places or moves a copy.
copy_staffs = true
# The chance a boss's corpse holds a copy staff, from 0 to 1.
copy_staff_boss_chance = 0.10
# The chance a chest that needs a key or a lockpick holds one.
copy_staff_locked_chest_chance = 0.02
";

const fn yes() -> bool {
    true
}

const fn boss_chance() -> f64 {
    0.10
}

const fn locked_chest_chance() -> f64 {
    0.02
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    per_altar: bool,
    #[serde(default = "yes")]
    copy_staffs: bool,
    #[serde(default = "boss_chance")]
    copy_staff_boss_chance: f64,
    #[serde(default = "locked_chest_chance")]
    copy_staff_locked_chest_chance: f64,
}

/// The mod's state.
#[derive(Debug)]
pub struct SurvivorsPerAltar;

impl Mod for SurvivorsPerAltar {
    #[expect(clippy::too_many_lines, reason = "one row per build")]
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "survivors-per-altar",
            side: BOTH,
            join = true,
            requires: ["flame-altar-zones" => "^0.1"],
            updates: github("themaiby/enshwick", "enshwick-survivors-per-altar"),
            builds: [
                {
                    side: SERVER,
                    guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
                    age: 387,
                    symbols: [
                        "survivors.spawn_return" => 0x4_05cb,
                        "survivors.unique_return_a" => 0x1f_4929,
                        "survivors.unique_return_b" => 0x1f_499d,
                        "survivors.bed_return" => 0x1f_29ff,
                        "survivors.bed_init_return" => 0x1f_2d76,
                        "survivors.cursor_return_a" => 0x9_9fe8,
                        "survivors.cursor_return_b" => 0x9_9ff3,
                        "survivors.assign_return_a" => 0xb_f541,
                        "survivors.assign_return_b" => 0xb_f5e4,
                        "survivors.roaming_return" => 0x1f_24f8,
                        "survivors.spawn" => 0x3_e170,
                        "survivors.registry_load" => 0x95_f900,
                        "survivors.map_find_slot" => 0x94_4290,
                    ],
                    constants: [
                        "survivors.unique_context" => 0x38,
                        "survivors.unique_saved_state" => 0x40,
                        "survivors.unique_transform" => 0x48,
                        "survivors.bed_context" => 0x40,
                        "survivors.bed_transform" => 0x48,
                        "survivors.bed_npc_common_type" => 0xa0,
                        "survivors.npc_common_bed" => 0x48,
                        "survivors.assign_context" => 0x30,
                        "survivors.assign_targets" => 0x40,
                        "survivors.assign_bed" => 0x85c,
                        "survivors.roaming_context" => 0x40,
                        "survivors.spawn_target" => 0x18,
                        "survivors.move_survivor" => 0x450,
                        "survivors.npc_transform" => 0x168,
                        "survivors.npc_saved_state" => 0x1e0,
                        "survivors.npc_common" => 0x300,
                        "survivors.npc_state" => 0x450,
                        "survivors.spawn_template" => 0x08,
                        "survivors.spawn_used_item" => 0x98,
                        "survivors.spawn_ammo_item" => 0xac,
                        "survivors.spawn_consume" => 0xe0,
                        "survivors.registry_loaded" => 0x20,
                        "survivors.registry_item_map" => 0xa8,
                        "survivors.map_bits" => 0x08,
                        "survivors.map_capacity" => 0x10,
                        "survivors.map_keys" => 0x28,
                        "survivors.map_values" => 0x40,
                        "survivors.map_count" => 0x54,
                    ],
                },
                {
                    // The built-in server's twins: the same code, so the
                    // server constants hold. `cursor_return` is
                    // `cursor_return_a`.
                    side: CLIENT,
                    guid: "69f0e9573b32c24a8838d3fff550824c",
                    age: 410,
                    symbols: [
                        "survivors.cursor_return" => 0x29_63a8,
                        "survivors.spawn_return" => 0x22_368b,
                        "survivors.unique_return_a" => 0x45_b299,
                        "survivors.unique_return_b" => 0x45_b30d,
                        "survivors.bed_return" => 0x45_744f,
                        "survivors.bed_init_return" => 0x45_77c6,
                        "survivors.cursor_return_a" => 0x29_63a8,
                        "survivors.cursor_return_b" => 0x29_63b3,
                        "survivors.assign_return_a" => 0x2d_2f61,
                        "survivors.assign_return_b" => 0x2d_3004,
                        "survivors.roaming_return" => 0x45_6f48,
                        "survivors.spawn" => 0x22_1230,
                        "survivors.registry_load" => 0xea_a500,
                        "survivors.map_find_slot" => 0xe8_fa70,
                    ],
                    constants: [
                        "survivors.unique_context" => 0x38,
                        "survivors.unique_saved_state" => 0x40,
                        "survivors.unique_transform" => 0x48,
                        "survivors.bed_context" => 0x40,
                        "survivors.bed_transform" => 0x48,
                        "survivors.bed_npc_common_type" => 0xa0,
                        "survivors.npc_common_bed" => 0x48,
                        "survivors.assign_context" => 0x30,
                        "survivors.assign_targets" => 0x40,
                        "survivors.assign_bed" => 0x85c,
                        "survivors.roaming_context" => 0x40,
                        "survivors.spawn_target" => 0x18,
                        "survivors.move_survivor" => 0x450,
                        "survivors.npc_transform" => 0x168,
                        "survivors.npc_saved_state" => 0x1e0,
                        "survivors.npc_common" => 0x300,
                        "survivors.npc_state" => 0x450,
                        "survivors.spawn_template" => 0x08,
                        "survivors.spawn_used_item" => 0x98,
                        "survivors.spawn_ammo_item" => 0xac,
                        "survivors.spawn_consume" => 0xe0,
                        "survivors.registry_loaded" => 0x20,
                        "survivors.registry_item_map" => 0xa8,
                        "survivors.map_bits" => 0x08,
                        "survivors.map_capacity" => 0x10,
                        "survivors.map_keys" => 0x28,
                        "survivors.map_values" => 0x40,
                        "survivors.map_count" => 0x54,
                        "survivors.state_status" => 0x14,
                        "survivors.ui_map_bits" => 0x46_3e98,
                        "survivors.ui_map_capacity" => 0x46_3ea0,
                        "survivors.ui_map_keys" => 0x46_3eb8,
                        "survivors.ui_map_values" => 0x46_3ed0,
                        "survivors.ui_map_count" => 0x46_3ee4,
                        "survivors.ui_record_size" => 0x928,
                        "survivors.ui_record_status" => 0x880,
                    ],
                },
            ],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.settings(DEFAULTS)?;
        if !settings.per_altar {
            return Err(Off::new("per_altar is off in the settings"));
        }
        let staffs = settings.copy_staffs;
        if staffs {
            copy_staffs(load, &settings)?;
        }
        if load.host().build().side == Side::SERVER {
            server::link(load, staffs)?;
        } else {
            // The server half acts in the games this client hosts.
            let merged = match server::link(load, staffs) {
                Ok(()) => true,
                Err(off) => {
                    enshwick_api::warn!("copies stay off in hosted games: {off}");
                    false
                }
            };
            client::link(load, !merged)?;
            enshwick_api::info!(
                "client half linked; server half {}",
                if merged {
                    "acts while this client hosts"
                } else {
                    "off"
                }
            );
        }
        Ok(Self)
    }
}

/// Adds the copy staffs, files them with their survivors and puts them in
/// loot.
fn copy_staffs(load: &mut Load<'_>, settings: &Settings) -> Result<(), Off> {
    use enshwick_api::loot::{Drop, Source};

    let texts = load.texts(staffs::TEXTS)?;
    staffs::add(load, &texts)?;
    staffs::link(load)?;
    let items = staffs::copy_ids();
    for (chance, source) in [
        (settings.copy_staff_boss_chance, Source::bosses()),
        (
            settings.copy_staff_locked_chest_chance,
            Source::locked_chests(),
        ),
    ] {
        if chance > 0.0 {
            load.add_loot(Drop::one_of(&items, chance, source))?;
        }
    }
    enshwick_api::info!(
        "{} copy staffs; in loot from bosses at {}, locked chests at {}",
        items.len(),
        settings.copy_staff_boss_chance,
        settings.copy_staff_locked_chest_chance
    );
    Ok(())
}

enshwick_api::declare_mod!(SurvivorsPerAltar);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(DEFAULTS).unwrap();
        assert!(settings.per_altar && settings.copy_staffs);
        assert!((settings.copy_staff_boss_chance - 0.10).abs() < f64::EPSILON);
        let old: Settings = toml::from_str("per_altar = true\n").unwrap();
        assert!(old.copy_staffs);
        assert!((old.copy_staff_locked_chest_chance - 0.02).abs() < f64::EPSILON);
    }
}

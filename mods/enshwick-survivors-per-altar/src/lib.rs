//! A copy of each survivor at every Flame Altar: a cluster of N altars holds at
//! most N copies of a survivor, each copy stands inside a zone, works there and
//! sleeps in a bed of its own altar.
//!
//! A summoning staff used in an altar zone without a copy places one there; in
//! a zone with a copy it moves that copy. The original survivor, the one the
//! game's own record names, is never touched.

mod client;
mod copies;
mod server;

use enshwick_api::abi::Side;
use enshwick_api::{Load, Mod, Off};

const DEFAULTS: &str = "\
# A copy of each survivor at every altar, at most one per altar zone.
per_altar = true
";

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    per_altar: bool,
}

/// The mod's state.
#[derive(Debug)]
pub struct SurvivorsPerAltar;

impl Mod for SurvivorsPerAltar {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "survivors-per-altar",
            side: BOTH,
            join = true,
            requires: ["flame-altar-zones" => "^0.1"],
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
                    ],
                },
                {
                    side: CLIENT,
                    guid: "69f0e9573b32c24a8838d3fff550824c",
                    age: 410,
                    symbols: ["survivors.cursor_return" => 0x29_63a8],
                    constants: [
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
        if load.host().build().side == Side::SERVER {
            server::link(load)?;
        } else {
            client::link(load)?;
        }
        Ok(Self)
    }
}

enshwick_api::declare_mod!(SurvivorsPerAltar);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(DEFAULTS).unwrap();
        assert!(settings.per_altar);
    }
}

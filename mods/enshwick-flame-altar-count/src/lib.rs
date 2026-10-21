//! How many Flame Altars each Flame level allows, up to 32 in 16 bases.
//!
//! The table is a join value: a client joined to a server runs with the
//! server's. The platform raises the game's grouping to 32 altar zones in its
//! 16 bases; where it cannot, the table stays capped at 16. Other mods change
//! the limit at the extension point `flame-altar-count:limit`.

mod hold;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use enshwick_loader_api::abi::Side;
use enshwick_loader_api::altars::{
    RAISED_ALTAR_ZONES, VANILLA_ALTAR_ZONES, VANILLA_ALTARS_PER_FLAME_LEVEL,
};
use enshwick_loader_api::points::OwnedPoint;
use enshwick_loader_api::points::flame_altar_count::AltarLimit;
use enshwick_loader_api::{Load, Mod, Off};

/// The most altars with the base limit raised.
const CAP: u8 = RAISED_ALTAR_ZONES;
/// The most altars the vanilla grouping takes.
const VANILLA_CAP: u8 = VANILLA_ALTAR_ZONES;
/// The cap in force: [`CAP`] once the base limit is lifted.
static IN_FORCE: AtomicU8 = AtomicU8::new(VANILLA_CAP);

/// The extension point `flame-altar-count:limit`, once defined.
static LIMIT: OnceLock<OwnedPoint<AltarLimit>> = OnceLock::new();

/// The altars `flame_level` allows: `table`'s answer, or what other mods'
/// links make of it, between 1 and the cap in force.
fn limit(flame_level: u32, table: u8) -> u8 {
    let cap = IN_FORCE.load(Ordering::Relaxed);
    let Some(point) = LIMIT.get() else {
        return table;
    };
    point
        .decide(&AltarLimit { flame_level, cap }, table)
        .value_or(table)
        .clamp(1, cap)
}

/// Flame Altar count settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Altars allowed at each Flame level, from level 0; levels past the end use
    /// the last entry. At most 32, in at most 16 bases: altars whose zones touch
    /// share a base, and an altar that would start a 17th base is refused.
    /// Vanilla: [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16]
    /// A joined player runs with the server's table.
    #[config(sync, text = "config.altars")]
    altars_per_flame_level: Vec<u16>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            altars_per_flame_level: vec![2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32],
        }
    }
}

/// The table of altars per Flame level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltarTable {
    table: Vec<u8>,
}

impl AltarTable {
    /// A table from the settings: capped at `cap`, at least 1, the vanilla
    /// table when empty.
    #[must_use]
    pub fn new(levels: &[u16], cap: u8) -> Self {
        let table = if levels.is_empty() {
            VANILLA_ALTARS_PER_FLAME_LEVEL.to_vec()
        } else {
            levels
                .iter()
                .map(|&n| u8::try_from(n.clamp(1, u16::from(cap))).unwrap_or(cap))
                .collect()
        };
        Self { table }
    }

    /// The table from `settings`, warning about values above the cap in force.
    fn from_settings(settings: &Settings) -> Self {
        let cap = IN_FORCE.load(Ordering::Relaxed);
        if settings
            .altars_per_flame_level
            .iter()
            .any(|&n| n > u16::from(cap))
        {
            enshwick_loader_api::warn!("altars above {cap} lowered to {cap}: {}", cap_reason(cap));
        }
        if settings.altars_per_flame_level.contains(&0) {
            enshwick_loader_api::warn!("a level with 0 altars raised to 1: every level allows one");
        }
        let table = Self::new(&settings.altars_per_flame_level, cap);
        enshwick_loader_api::info!("altars per Flame level: {:?}", table.table);
        table
    }

    /// Altars allowed at `flame_level`.
    #[must_use]
    pub fn altars(&self, flame_level: u32) -> u8 {
        let index = usize::try_from(flame_level).unwrap_or(usize::MAX);
        self.table
            .get(index)
            .or(self.table.last())
            .copied()
            .unwrap_or(VANILLA_CAP)
    }
}

/// Why the table stops at `cap`.
fn cap_reason(cap: u8) -> &'static str {
    if cap == CAP {
        "a base names at most 32 altar zones"
    } else {
        "the game groups only 16 altar zones into bases here"
    }
}

/// The mod: the running table, swapped whole when the settings or the
/// server's join values change.
#[derive(Debug)]
pub struct AltarCount {
    table: ArcSwap<AltarTable>,
}

impl Mod for AltarCount {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "flame-altar-count",
            side: BOTH,
            name: "Flame altar count",
            join = true,
            updates: github("themaiby/enshwick", "enshwick-flame-altar-count"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let server = load.host().build().side == Side::SERVER;
        if load.raise_base_limit()? {
            IN_FORCE.store(CAP, Ordering::Relaxed);
            if server {
                hold::keep(load)?;
            }
        } else {
            // A world held for its altars must not open with the vanilla
            // grouping: off, the loader stops the server for the hold.
            if server && let Some(reason) = hold::reason(load)? {
                return Err(Off::new(format!(
                    "{reason}, and this build's grouping is not the one the platform knows"
                )));
            }
            enshwick_loader_api::warn!("the game keeps 16 altar zones in bases");
        }
        let table = AltarTable::from_settings(&settings);
        let _first = LIMIT.set(load.define_point()?);
        load.on_altar_limit(|level| {
            let table = AltarCount::instance()
                .map_or(VANILLA_CAP, |count| count.table.load().altars(level));
            limit(level, table)
        })?;
        load.on_config_change(|settings: Settings| {
            if let Some(count) = AltarCount::instance() {
                count
                    .table
                    .store(Arc::new(AltarTable::from_settings(&settings)));
            }
        })?;
        Ok(Self {
            table: ArcSwap::from_pointee(table),
        })
    }
}

enshwick_loader_api::declare_mod!(AltarCount);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::config::Config as _;

    use super::*;

    #[test]
    fn the_default_settings_double_vanilla() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        let table = AltarTable::new(&settings.altars_per_flame_level, CAP);
        assert_eq!(table.altars(0), 2);
        assert_eq!(table.altars(1), 4);
        assert_eq!(table.altars(4), 16);
        let doubled: Vec<u8> = VANILLA_ALTARS_PER_FLAME_LEVEL
            .iter()
            .map(|&n| n * 2)
            .collect();
        assert_eq!(table.table, doubled);
        assert_eq!(table.altars(99), 32);
    }

    #[test]
    fn values_are_capped_and_an_empty_table_is_vanilla() {
        let table = AltarTable::new(&[0, 3, 40], CAP);
        assert_eq!(
            [table.altars(0), table.altars(1), table.altars(2)],
            [1, 3, 32]
        );
        assert_eq!(AltarTable::new(&[], CAP).altars(0), 1);
        let unlifted = AltarTable::new(&[0, 3, 40], VANILLA_CAP);
        assert_eq!(unlifted.altars(2), 16);
    }

    #[test]
    fn the_table_is_the_join_value() {
        let synced = Settings::default().synced().unwrap();
        assert_eq!(
            synced,
            "{ altars_per_flame_level = [2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32] }"
        );
        let mut local: Settings = toml::from_str("altars_per_flame_level = [1]").unwrap();
        local.merge_synced(&synced).unwrap();
        assert_eq!(
            AltarTable::new(&local.altars_per_flame_level, CAP).altars(3),
            12
        );
    }

    #[test]
    fn the_table_answers_the_limit_with_the_base_limit_raised() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .side(Side::CLIENT)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick_loader_api::abi::Status::OK);
        assert!(loader.base_limit_raised());
        assert_eq!(loader.altar_limit(1), Some(4));
        assert_eq!(loader.altar_limit(11), Some(32));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn other_mods_change_the_limit_within_the_cap() {
        use enshwick_loader_api::points::Verdict;

        let loader = enshwick_loader_api::test::FakeHost::new()
            .side(Side::CLIENT)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick_loader_api::abi::Status::OK);
        assert_eq!(loader.defined_points(), ["flame-altar-count:limit"]);
        assert!(loader.extend_point(|limit: &AltarLimit, altars: &u8| {
            assert_eq!(limit.cap, CAP);
            match limit.flame_level {
                0 => Verdict::Set(0),
                1 => Verdict::Set(altars + 2),
                _ => Verdict::Set(200),
            }
        }));
        assert_eq!(loader.altar_limit(0), Some(1), "at least one");
        assert_eq!(loader.altar_limit(1), Some(6));
        assert_eq!(loader.altar_limit(5), Some(32), "at most the cap");
        assert!(loader.faults().is_empty());
    }
}

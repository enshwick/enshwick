//! How many Flame Altars each Flame level allows, up to 32 in 16 bases.
//!
//! The game asks one function, `max_altars(balancing, flame_level)`, which
//! reads `altarsPerFlameLevel` and caps it at 16. The cursor check that refuses
//! an altar past the limit and the client's interface both ask it, so the hook
//! answers from the settings table instead. The table is a join value: a
//! client joined to a server runs with the server's.
//!
//! The game groups only 16 altar zones into bases; [`bases`] lifts that to 32
//! zones and keeps the bases at 16. When its code is not found, the table
//! stays capped at 16.

mod bases;
mod limit;

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use arc_swap::ArcSwap;
use enshwick_loader_api::{Load, Mod, Next, Off};

/// Vanilla `altarsPerFlameLevel` for Flame levels 0 to 11; later levels repeat
/// the last entry.
const VANILLA: [u8; 12] = [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16];
/// The most altars with the base limit lifted: a base names its zones in a
/// 32-bit mask.
const CAP: u8 = 32;
/// The most altars the vanilla grouping takes.
const VANILLA_CAP: u8 = 16;
/// The cap in force: [`CAP`] once the base limit is lifted.
static IN_FORCE: AtomicU8 = AtomicU8::new(VANILLA_CAP);

/// Flame Altar count settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Altars allowed at each Flame level, from level 0; levels past the end use
    /// the last entry. At most 32, in at most 16 bases: altars whose zones touch
    /// share a base, and an altar that would start a 17th base is refused.
    /// Vanilla: [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16]
    /// A joined player runs with the server's table.
    #[config(sync)]
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
            VANILLA.to_vec()
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

enshwick_loader_api::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, flame_level: u32) -> u8 = max_altars;
}

fn max_altars(
    next: Next<unsafe extern "system" fn(u64, u32) -> u8>,
    balancing: u64,
    flame_level: u32,
) -> u8 {
    match AltarCount::instance() {
        Some(count) => count.table.load().altars(flame_level),
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.get()(balancing, flame_level) },
    }
}

impl Mod for AltarCount {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "flame-altar-count",
            side: BOTH,
            join = true,
            updates: github("themaiby/enshwick", "enshwick-flame-altar-count"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        if bases::link(load)? {
            IN_FORCE.store(CAP, Ordering::Relaxed);
        }
        let table = AltarTable::from_settings(&settings);
        load.hook(&MAX_ALTARS, load.symbol("building.max_altars")?)?;
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
        let doubled: Vec<u8> = VANILLA.iter().map(|&n| n * 2).collect();
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
}

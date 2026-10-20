//! How many Flame Altars each Flame level allows.
//!
//! The game asks one function, `max_altars(balancing, flame_level)`, which
//! reads `altarsPerFlameLevel` and caps it at 16. The cursor check that refuses
//! an altar past the limit and the client's interface both ask it, so the hook
//! answers from the settings table instead. The table is a join value: a
//! client joined to a server runs with the server's.

use std::sync::Arc;

use arc_swap::ArcSwap;
use enshwick_api::{Load, Mod, Next, Off};

/// Vanilla `altarsPerFlameLevel` for Flame levels 0 to 11; later levels repeat
/// the last entry.
const VANILLA: [u8; 12] = [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16];
/// The game groups only the first 16 zones of a world into bases.
const CAP: u8 = 16;

/// Flame Altar count settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Altars allowed at each Flame level, from level 0; levels past the end use
    /// the last entry. At most 16: the game groups only 16 altar zones into bases.
    /// Vanilla: [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16]
    /// A joined player runs with the server's table.
    #[config(sync)]
    altars_per_flame_level: Vec<u16>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            altars_per_flame_level: vec![2, 4, 8, 12, 16, 16, 16, 16, 16, 16, 16, 16],
        }
    }
}

/// The table of altars per Flame level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltarTable {
    table: Vec<u8>,
}

impl AltarTable {
    /// A table from the settings: capped at 16, at least 1, the vanilla table
    /// when empty.
    #[must_use]
    pub fn new(levels: &[u16]) -> Self {
        let table = if levels.is_empty() {
            VANILLA.to_vec()
        } else {
            levels
                .iter()
                .map(|&n| u8::try_from(n.clamp(1, u16::from(CAP))).unwrap_or(CAP))
                .collect()
        };
        Self { table }
    }

    /// The table from `settings`, warning about values above the cap.
    fn from_settings(settings: &Settings) -> Self {
        if settings
            .altars_per_flame_level
            .iter()
            .any(|&n| n > u16::from(CAP))
        {
            enshwick_api::warn!(
                "altars above {CAP} lowered to {CAP}: the game groups only {CAP} zones into bases"
            );
        }
        let table = Self::new(&settings.altars_per_flame_level);
        enshwick_api::info!("altars per Flame level: {:?}", table.table);
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
            .unwrap_or(CAP)
    }
}

/// The mod: the running table, swapped whole when the settings or the
/// server's join values change.
#[derive(Debug)]
pub struct AltarCount {
    table: ArcSwap<AltarTable>,
}

enshwick_api::detour! {
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
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "flame-altar-count",
            side: BOTH,
            join = true,
            updates: github("themaiby/enshwick", "enshwick-flame-altar-count"),
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
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

enshwick_api::declare_mod!(AltarCount);

#[cfg(test)]
mod tests {
    use enshwick_api::config::Config as _;

    use super::*;

    #[test]
    fn the_default_settings_double_vanilla_up_to_the_cap() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        let table = AltarTable::new(&settings.altars_per_flame_level);
        assert_eq!(table.altars(0), 2);
        assert_eq!(table.altars(1), 4);
        assert_eq!(table.altars(4), 16);
        assert_eq!(table.altars(99), 16);
    }

    #[test]
    fn values_are_capped_and_an_empty_table_is_vanilla() {
        let table = AltarTable::new(&[0, 3, 40]);
        assert_eq!(
            [table.altars(0), table.altars(1), table.altars(2)],
            [1, 3, 16]
        );
        assert_eq!(AltarTable::new(&[]).altars(0), 1);
    }

    #[test]
    fn the_table_is_the_join_value() {
        let synced = Settings::default().synced().unwrap();
        assert_eq!(
            synced,
            "{ altars_per_flame_level = [2, 4, 8, 12, 16, 16, 16, 16, 16, 16, 16, 16] }"
        );
        let mut local: Settings = toml::from_str("altars_per_flame_level = [1]").unwrap();
        local.merge_synced(&synced).unwrap();
        assert_eq!(AltarTable::new(&local.altars_per_flame_level).altars(3), 12);
    }
}

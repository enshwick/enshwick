//! How many Flame Altars each Flame level allows.
//!
//! The game asks one function, `max_altars(balancing, flame_level)`, which
//! reads `altarsPerFlameLevel` and caps it at 16. The cursor check that refuses
//! an altar past the limit and the client's interface both ask it, so the hook
//! answers from the settings table instead.

use enshwick_api::{Load, Mod, Next, Off};

/// Vanilla `altarsPerFlameLevel` for Flame levels 0 to 11; later levels repeat
/// the last entry.
const VANILLA: [u8; 12] = [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16];
/// The game groups only the first 16 zones of a world into bases.
const CAP: u8 = 16;

const DEFAULTS: &str = "\
# Altars allowed at each Flame level, from level 0; levels past the end use
# the last entry. At most 16: the game groups only 16 altar zones into bases.
# Vanilla: [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16]
altars_per_flame_level = [2, 4, 8, 12, 16, 16, 16, 16, 16, 16, 16, 16]
";

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    altars_per_flame_level: Vec<u16>,
}

/// The table of altars per Flame level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltarCount {
    table: Vec<u8>,
}

impl AltarCount {
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

enshwick_api::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, flame_level: u32) -> u8 = max_altars;
}

fn max_altars(
    next: Next<unsafe extern "system" fn(u64, u32) -> u8>,
    balancing: u64,
    flame_level: u32,
) -> u8 {
    match AltarCount::instance() {
        Some(count) => count.altars(flame_level),
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.get()(balancing, flame_level) },
    }
}

impl Mod for AltarCount {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(id: "flame-altar-count", side: BOTH, join = true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.settings(DEFAULTS)?;
        if settings
            .altars_per_flame_level
            .iter()
            .any(|&n| n > u16::from(CAP))
        {
            enshwick_api::warn!(
                "altars above {CAP} lowered to {CAP}: the game groups only {CAP} zones into bases"
            );
        }
        let count = Self::new(&settings.altars_per_flame_level);
        load.hook(&MAX_ALTARS, load.symbol("building.max_altars")?)?;
        enshwick_api::info!("altars per Flame level: {:?}", count.table);
        Ok(count)
    }
}

enshwick_api::declare_mod!(AltarCount);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_double_vanilla_up_to_the_cap() {
        let settings: Settings = toml_settings(DEFAULTS);
        let count = AltarCount::new(&settings.altars_per_flame_level);
        assert_eq!(count.altars(0), 2);
        assert_eq!(count.altars(1), 4);
        assert_eq!(count.altars(4), 16);
        assert_eq!(count.altars(99), 16);
    }

    #[test]
    fn values_are_capped_and_an_empty_table_is_vanilla() {
        let count = AltarCount::new(&[0, 3, 40]);
        assert_eq!(
            [count.altars(0), count.altars(1), count.altars(2)],
            [1, 3, 16]
        );
        assert_eq!(AltarCount::new(&[]).altars(0), 1);
    }

    fn toml_settings(text: &str) -> Settings {
        toml::from_str(text).unwrap()
    }
}

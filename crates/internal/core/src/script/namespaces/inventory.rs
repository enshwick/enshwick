//! The inventory domain's namespaces, and how `Load` opens each of its
//! handles.

use super::{Bound, Function, Namespace, Object};
use crate::affixes::{AffixFns, Affixes};
use crate::effects::Effects;
use crate::inventory::Inventories;
use crate::loot::LootFns;
use crate::stats::Stats;
use crate::survivors::SurvivorFns;
use crate::{Load, Off};

pub(super) const NAMESPACES: &[Namespace] = &[
    Namespace::of::<Inventories>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("damage", "", &[Load::SCRIPT_DAMAGE_FILTER]),
    Namespace::of::<Effects>(),
    Namespace::of::<SurvivorFns>(),
    Namespace::starting(
        "survivors",
        "",
        &[Load::SCRIPT_SURVIVORS, Load::SCRIPT_SURVIVOR_HOOKS],
    ),
    #[cfg(feature = "unstable")]
    Namespace::starting("item_data", "", &[Load::SCRIPT_ITEM_DATA]),
    #[cfg(feature = "unstable")]
    Namespace::starting("entity_data", "", &[Load::SCRIPT_ENTITY_DATA]),
    #[cfg(feature = "unstable")]
    Namespace::starting("items", "", &[Load::SCRIPT_ITEMS]),
    Namespace::of::<Affixes>(),
    Namespace::of::<AffixFns>(),
    Namespace::starting(
        "affixes",
        "",
        &[
            Load::SCRIPT_AFFIX_DEFS,
            #[cfg(feature = "unstable")]
            Load::SCRIPT_AFFIX_ROLLS,
        ],
    ),
    Namespace::of::<LootFns>(),
    Namespace::starting(
        "loot",
        "",
        &[Load::SCRIPT_LOOT_DROPS, Load::SCRIPT_LOOT_PASS],
    ),
    Namespace::on_request::<Stats>("stats"),
    Namespace::starting("stats", "", &[Load::SCRIPT_STATS]),
];

impl Bound for Inventories {
    const NAME: &'static str = "inventory";
    const DOC: &'static str =
        "Inventories on the server: their slots and counts, and items given, taken and moved.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_STOCK, Self::SCRIPT_READS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.inventories()
    }
}

impl Bound for Effects {
    const NAME: &'static str = "effects";
    const DOC: &'static str = "Effects a mod has on the server's world: survivor orders, toggles, threat, pings, teleports and spawns.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_EVENTS, Self::SCRIPT_SPAWNS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.effects())
    }
}

impl Bound for SurvivorFns {
    const NAME: &'static str = "survivors";
    const DOC: &'static str = "Survivors: the game's lookups of them, their beds, summoning staffs and the workshop survivors.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_SURVIVOR_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for Affixes {
    const NAME: &'static str = "affixes";
    const DOC: &'static str =
        "Affixes: bundles of stat modifiers rolled onto single items, their lines and rolls.";
    const PARTS: &'static [&'static [Function<Self>]] = &[
        Self::SCRIPT_AFFIXES,
        #[cfg(feature = "unstable")]
        Self::SCRIPT_ROLLING,
    ];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.affixes())
    }
}

impl Bound for AffixFns {
    const NAME: &'static str = "affixes";
    const DOC: &'static str = <Affixes as Bound>::DOC;
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_AFFIX_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for LootFns {
    const NAME: &'static str = "loot";
    const DOC: &'static str = "Mod items in random loot, and the game's loot passes.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LOOT_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for Stats {
    const NAME: &'static str = "stats";
    const DOC: &'static str =
        "Stats of entities and items: values, modifiers, sources and definitions.";
    const PARTS: &'static [&'static [Function<Self>]] = <Self as Object>::PARTS;

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.stats()
    }
}

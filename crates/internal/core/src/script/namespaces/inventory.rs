//! The inventory domain's namespaces.

use super::Namespace;
use crate::Load;
use crate::affixes::{AffixFns, Affixes};
use crate::effects::Effects;
use crate::inventory::Inventories;
use crate::loot::LootFns;
use crate::stats::Stats;
use crate::survivors::SurvivorFns;

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

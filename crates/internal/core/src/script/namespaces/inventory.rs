//! The inventory domain's namespaces.

use super::Namespace;

pub(super) const NAMESPACES: &[Namespace] = &[
    Namespace::of::<crate::inventory::Inventories>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("damage", "", &[crate::Load::SCRIPT_DAMAGE_FILTER]),
    Namespace::of::<crate::effects::Effects>(),
    Namespace::of::<crate::survivors::SurvivorFns>(),
    Namespace::starting(
        "survivors",
        "",
        &[
            crate::Load::SCRIPT_SURVIVORS,
            crate::Load::SCRIPT_SURVIVOR_HOOKS,
        ],
    ),
    #[cfg(feature = "unstable")]
    Namespace::starting("item_data", "", &[crate::Load::SCRIPT_ITEM_DATA]),
    #[cfg(feature = "unstable")]
    Namespace::starting("entity_data", "", &[crate::Load::SCRIPT_ENTITY_DATA]),
    #[cfg(feature = "unstable")]
    Namespace::starting("items", "", &[crate::Load::SCRIPT_ITEMS]),
    Namespace::of::<crate::affixes::Affixes>(),
    Namespace::of::<crate::affixes::AffixFns>(),
    Namespace::starting(
        "affixes",
        "",
        &[
            crate::Load::SCRIPT_AFFIX_DEFS,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_AFFIX_ROLLS,
        ],
    ),
    Namespace::of::<crate::loot::LootFns>(),
    Namespace::starting(
        "loot",
        "",
        &[
            crate::Load::SCRIPT_LOOT_DROPS,
            crate::Load::SCRIPT_LOOT_PASS,
        ],
    ),
    Namespace::on_request::<crate::stats::Stats>("stats"),
    Namespace::starting("stats", "", &[crate::Load::SCRIPT_STATS]),
];

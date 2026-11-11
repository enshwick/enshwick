//! The knowledge domain's namespaces.

use super::Namespace;

pub(super) const NAMESPACES: &[Namespace] = &[
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::rewards::Rewards>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("rewards", "", &[crate::Load::SCRIPT_REWARDS]),
    Namespace::starting(
        "progress",
        "",
        &[
            crate::Load::SCRIPT_PROGRESS,
            crate::Load::SCRIPT_PROGRESS_HOOKS,
        ],
    ),
    Namespace::of::<crate::knowledge::Knowledge>(),
    Namespace::starting("knowledge", "", &[crate::Load::SCRIPT_KNOWLEDGE]),
    #[cfg(feature = "typed")]
    Namespace::of::<crate::quests::Quests>(),
];

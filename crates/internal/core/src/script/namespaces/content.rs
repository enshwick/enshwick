//! The content domain's namespaces.

use super::Namespace;

pub(super) const NAMESPACES: &[Namespace] = &[
    #[cfg(feature = "typed")]
    Namespace::starting(
        "typed",
        "The game's components, events and resources as the typed API generates them from its reflection.",
        &[crate::Load::SCRIPT_TYPED],
    ),
    Namespace::of::<crate::rates::RateFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("rates", "", &[crate::Load::SCRIPT_RATES]),
    Namespace::of::<crate::content::Content>(),
    Namespace::starting(
        "content",
        "",
        &[
            crate::Load::SCRIPT_CONTENT_IDS,
            crate::Load::SCRIPT_CONTENT_ITEMS,
            crate::Load::SCRIPT_CONTENT_RECIPES,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_CONTENT_RECIPE_WITH,
            crate::Load::SCRIPT_CONTENT_WORKSHOPS,
            crate::Load::SCRIPT_CONTENT_TEMPLATES,
            crate::Load::SCRIPT_CONTENT_PROPS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_ENEMIES,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_SPAWNS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_KNOWLEDGE,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_JOURNAL,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_SKILLS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_CONTENT_REMOVAL,
            crate::Load::SCRIPT_CONTENT_ICONS,
            crate::Load::SCRIPT_CONTENT_RENAMES,
            crate::Load::SCRIPT_CONTENT_CHANGE,
            crate::Load::SCRIPT_CONTENT_CHANGE_ITEMS,
            crate::Load::SCRIPT_CONTENT_CHANGE_RECIPES,
            crate::Load::SCRIPT_CONTENT_LIVE,
        ],
    ),
];

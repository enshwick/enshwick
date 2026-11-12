//! The content domain's namespaces.

use super::Namespace;
use crate::Load;
use crate::content::Content;
use crate::rates::RateFns;

pub(super) const NAMESPACES: &[Namespace] = &[
    Namespace::of::<RateFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("rates", "", &[Load::SCRIPT_RATES]),
    Namespace::of::<Content>(),
    Namespace::starting(
        "content",
        "",
        &[
            Load::SCRIPT_CONTENT_IDS,
            Load::SCRIPT_CONTENT_ITEMS,
            Load::SCRIPT_CONTENT_RECIPES,
            #[cfg(feature = "unstable")]
            Load::SCRIPT_CONTENT_RECIPE_WITH,
            Load::SCRIPT_CONTENT_WORKSHOPS,
            Load::SCRIPT_CONTENT_TEMPLATES,
            Load::SCRIPT_CONTENT_PROPS,
            #[cfg(feature = "typed")]
            Load::SCRIPT_CONTENT_ENEMIES,
            #[cfg(feature = "typed")]
            Load::SCRIPT_CONTENT_SPAWNS,
            #[cfg(feature = "typed")]
            Load::SCRIPT_CONTENT_KNOWLEDGE,
            #[cfg(feature = "typed")]
            Load::SCRIPT_CONTENT_JOURNAL,
            #[cfg(feature = "typed")]
            Load::SCRIPT_CONTENT_SKILLS,
            #[cfg(feature = "typed")]
            Load::SCRIPT_CONTENT_REMOVAL,
            Load::SCRIPT_CONTENT_ICONS,
            Load::SCRIPT_CONTENT_RENAMES,
            Load::SCRIPT_CONTENT_CHANGE,
            Load::SCRIPT_CONTENT_CHANGE_ITEMS,
            Load::SCRIPT_CONTENT_CHANGE_RECIPES,
            Load::SCRIPT_CONTENT_LIVE,
        ],
    ),
];

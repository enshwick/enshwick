//! The content domain's namespaces, and how `Load` opens each of its
//! handles.

use super::{Bound, Function, Namespace};
use crate::content::Content;
use crate::rates::RateFns;
use crate::{Load, Off};

pub(super) const NAMESPACES: &[Namespace] = &[
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

impl Bound for RateFns {
    const NAME: &'static str = "rates";
    const DOC: &'static str =
        "The game's rates, such as experience and drops, each mod's factor on them combined.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_RATE_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for Content {
    const NAME: &'static str = "content";
    const DOC: &'static str =
        "Game content from mods: new items, recipes and workshops, and changes to vanilla ones.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_CONTENT_LOOKUPS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

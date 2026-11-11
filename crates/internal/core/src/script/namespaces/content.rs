//! The content domain's namespaces, and how `Load` opens each of its
//! handles.

use super::{Bound, Function, Namespace};
use crate::content::Content;
use crate::rates::RateFns;
use crate::{Load, Off};

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

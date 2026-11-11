//! The base's namespaces, and how `Load` opens each handle of the base.

use super::{Bound, Function, Log, Namespace};
use crate::{Load, Off};

pub(super) const NAMESPACES: &[Namespace] = &[
    Namespace::of::<Log>(),
    Namespace::of::<crate::schedule::Scheduler>(),
    Namespace::of::<crate::resources::Resources>(),
    Namespace::of::<crate::mods::Mods>(),
    Namespace::of::<crate::net::NetFns>(),
    Namespace::starting("net", "", &[crate::Load::SCRIPT_NET]),
    Namespace::starting("pacing", "", &[crate::Load::SCRIPT_PACING]),
    Namespace::of::<crate::ecs::EcsFns>(),
    Namespace::starting(
        "",
        "",
        &[
            crate::Load::SCRIPT_STATE,
            crate::Load::SCRIPT_TICKS,
            crate::Load::SCRIPT_DIRS,
        ],
    ),
    Namespace::of::<crate::state::StateFns>(),
    Namespace::of::<crate::files::FilesFns>(),
    Namespace::starting("status", "", &[crate::Load::SCRIPT_STATUS]),
    Namespace::of::<crate::i18n::I18nFns>(),
    Namespace::starting("i18n", "", &[crate::Load::SCRIPT_I18N]),
];

impl Bound for crate::schedule::Scheduler {
    const NAME: &'static str = "schedule";
    const DOC: &'static str =
        "Work for the server's game thread: on the next pass, after a delay, or repeatedly.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_TASKS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.scheduler()
    }
}

impl Bound for crate::resources::Resources {
    const NAME: &'static str = "resources";
    const DOC: &'static str =
        "The game's types, its resources by type and as values, and resources served from an edit.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LOOKUPS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self::open(load.host()))
    }
}

impl Bound for crate::mods::Mods {
    const NAME: &'static str = "mods";
    const DOC: &'static str =
        "The loaded mods, the side the mod runs on, and its own folder, read-only.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_FOLDER];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Self::open(load.base())
    }
}

impl Bound for crate::net::NetFns {
    const NAME: &'static str = "net";
    const DOC: &'static str = "Messages between a mod's server part and its client part.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_NET_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for crate::ecs::EcsFns {
    const NAME: &'static str = "ecs";
    const DOC: &'static str = "The world's fixed-point coordinates and metres.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_ECS_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for crate::state::StateFns {
    const NAME: &'static str = "state";
    const DOC: &'static str = "The data a mod keeps for a world, and holds on its saves.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_STATE_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for crate::files::FilesFns {
    const NAME: &'static str = "files";
    const DOC: &'static str =
        "Files a mod writes whole, such as exports for tools outside the game.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_FILES_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

impl Bound for crate::i18n::I18nFns {
    const NAME: &'static str = "i18n";
    const DOC: &'static str = "The game's languages and texts, and a mod's own translations.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_I18N_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

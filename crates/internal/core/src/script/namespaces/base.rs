//! The base's namespaces.

use super::{Log, Namespace};

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

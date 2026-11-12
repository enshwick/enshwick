//! The base's namespaces.

use super::{Log, Namespace};
use crate::Load;

pub(super) const NAMESPACES: &[Namespace] = &[
    Namespace::of::<Log>(),
    Namespace::of::<crate::schedule::Scheduler>(),
    Namespace::of::<crate::resources::Resources>(),
    Namespace::of::<crate::mods::Mods>(),
    Namespace::of::<crate::net::NetFns>(),
    Namespace::starting("net", "", &[Load::SCRIPT_NET]),
    Namespace::starting("pacing", "", &[Load::SCRIPT_PACING]),
    Namespace::of::<crate::ecs::EcsFns>(),
    Namespace::starting(
        "",
        "",
        &[Load::SCRIPT_STATE, Load::SCRIPT_TICKS, Load::SCRIPT_DIRS],
    ),
    Namespace::of::<crate::state::StateFns>(),
    Namespace::of::<crate::files::FilesFns>(),
    Namespace::starting("status", "", &[Load::SCRIPT_STATUS]),
    Namespace::of::<crate::i18n::I18nFns>(),
    Namespace::starting("i18n", "", &[Load::SCRIPT_I18N]),
];

//! The actions domain's namespaces.

use super::Namespace;

pub(super) const NAMESPACES: &[Namespace] = &[
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::actions::ActionFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("actions", "", &[crate::Load::SCRIPT_ACTIONS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::controls::ControlFns>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("controls", "", &[crate::Load::SCRIPT_CONTROLS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::console::ConsoleFns>(),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::commands::CommandFns>(),
    Namespace::starting(
        "vetoes",
        "",
        &[crate::Load::SCRIPT_VETOES, crate::Load::SCRIPT_VETOES_BUILD],
    ),
    Namespace::starting("chat", "", &[crate::Load::SCRIPT_CHAT]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::interactions::Targets>(),
    Namespace::starting("interactions", "", &[crate::Load::SCRIPT_INTERACTIONS]),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::input::Input>(),
];

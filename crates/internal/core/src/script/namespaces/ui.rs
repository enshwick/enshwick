//! The ui domain's namespaces.

use super::Namespace;

pub(super) const NAMESPACES: &[Namespace] = &[
    Namespace::of::<crate::ui::Ui>(),
    Namespace::starting(
        "ui",
        "",
        &[
            crate::Load::SCRIPT_UI,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_UI_EXTENSION,
        ],
    ),
    #[cfg(feature = "unstable")]
    Namespace::of_starting::<crate::sound::Sounds>(&[
        crate::Load::SCRIPT_SOUND,
        crate::Load::SCRIPT_SOUND_EVENTS,
        crate::Load::SCRIPT_SOUND_ADD,
    ]),
    #[cfg(not(feature = "unstable"))]
    Namespace::starting(
        "sound",
        "",
        &[crate::Load::SCRIPT_SOUND, crate::Load::SCRIPT_SOUND_EVENTS],
    ),
];

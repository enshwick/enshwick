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
    Namespace::starting(
        "sound",
        "",
        &[
            crate::Load::SCRIPT_SOUND,
            #[cfg(feature = "unstable")]
            crate::Load::SCRIPT_SOUND_ADD,
        ],
    ),
];

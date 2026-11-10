//! Where the looks the loader draws with sit in the game's look bundle, read
//! by reflection name: label styles, sounds, setting texts and icons.

use crate::host::Loader;

/// Where the looks the loader uses sit in the look bundle, from reflection.
#[derive(Debug, Clone, Copy)]
pub(super) struct Looks {
    /// Label styles, by `UiWidget::STYLE_*`.
    pub(super) styles: [usize; 8],
    pub(super) title: usize,
    pub(super) title_size: usize,
    pub(super) outline: usize,
    /// Sounds, by `UiWidget::SOUND_*`.
    pub(super) sounds: [usize; 4],
    pub(super) setting_off: usize,
    pub(super) setting_on: usize,
    /// The bundle's icons, by [`Icon`]; `None` keeps buttons to text.
    pub(super) icons: Option<[usize; 2]>,
    /// The font size of a list entry's name, the bundle's `large`; `None`
    /// keeps names at their style's size.
    pub(super) name_size: Option<usize>,
}

/// An icon of the game's own look bundle that a button can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum Icon {
    /// The gear of the game's own settings buttons.
    Settings = 0,
    /// The circled i the game puts beside details.
    Info = 1,
}

/// The bundle offsets of the [`Icon`]s, from reflection, in their order;
/// `None`, logged once at start, keeps icon buttons to their text.
fn icons(loader: &Loader) -> Option<[usize; 2]> {
    let found = generic_icons(loader);
    if found.is_none() {
        crate::log!(
            WARN,
            "interface: icon buttons show their text on {}: the look bundle's icons do not read",
            loader.build.name
        );
    }
    found
}

fn generic_icons(loader: &Loader) -> Option<[usize; 2]> {
    let generic = |name| {
        path(
            loader,
            &[
                (BUNDLE, "icons"),
                ("keen::FbUiIcons", "genericIcons"),
                ("keen::FbUiGenericIcons", name),
            ],
        )
    };
    Some([generic("settings")?, generic("infoCircle")?])
}

/// Where the look of the main menu's version line sits in the look bundle,
/// from reflection.
#[derive(Debug, Clone, Copy)]
pub(super) struct Legal {
    pub(super) style: usize,
    pub(super) size: usize,
}

impl Legal {
    pub(super) fn find(loader: &Loader) -> Option<Self> {
        Some(Self {
            style: style(loader, "legalText")?,
            size: path(loader, &[(BUNDLE, "fontSizes"), (FONT_SIZES, "xsmall")])?,
        })
    }
}

/// The bundle offset of a reflected path, `(type, field)` pairs summed.
pub(super) fn path(loader: &Loader, steps: &[(&str, &str)]) -> Option<usize> {
    steps.iter().try_fold(0usize, |sum, (ty, field)| {
        let offset = loader.reflection.field_offset(ty, field).ok()?;
        Some(sum + usize::try_from(offset).ok()?)
    })
}

pub(super) const BUNDLE: &str = "keen::FbUiBundle";
const STYLES: &str = "keen::FbUiCharacterStyles";
const SOUNDS: &str = "keen::FbUiSounds";
const FONT_SIZES: &str = "keen::FbUiFontSizes";
const MENU_SOUNDS: &str = "keen::FbUiSoundsMenuGeneric";

fn style(loader: &Loader, name: &str) -> Option<usize> {
    path(loader, &[(BUNDLE, "namedStyles"), (STYLES, name)])
}

pub(super) fn sound(loader: &Loader, name: &str) -> Option<usize> {
    path(
        loader,
        &[(BUNDLE, "sounds"), (SOUNDS, "menu"), (MENU_SOUNDS, name)],
    )
}

fn setting_text(loader: &Loader, name: &str) -> Option<usize> {
    path(
        loader,
        &[
            (BUNDLE, "locatags"),
            ("keen::FbUiLocaLinks", "menuStrings"),
            ("keen::FbUiLocaMenus", "settings"),
            ("keen::FbUiLocaMenuSettings", name),
        ],
    )
}

pub(super) fn looks(loader: &Loader) -> Option<Looks> {
    Some(Looks {
        styles: [
            style(loader, "defaultStyle")?,
            style(loader, "secondaryText")?,
            style(loader, "h1")?,
            style(loader, "h2")?,
            style(loader, "h3")?,
            style(loader, "warning")?,
            style(loader, "greenText")?,
            style(loader, "redText")?,
        ],
        title: path(
            loader,
            &[
                (BUNDLE, "namedStyles"),
                (STYLES, "menu"),
                ("keen::FbUiMenuCharacterStyles", "header1"),
            ],
        )?,
        outline: style(loader, "defaultStyleOutline")?,
        title_size: path(loader, &[(BUNDLE, "fontSizes"), (FONT_SIZES, "caps_large")])?,
        sounds: [
            sound(loader, "mouseClickGeneric")?,
            sound(loader, "entryTrigger")?,
            sound(loader, "back")?,
            sound(loader, "entryTriggerFailed")?,
        ],
        setting_off: setting_text(loader, "settingOff")?,
        setting_on: setting_text(loader, "settingOn")?,
        icons: icons(loader),
        name_size: path(loader, &[(BUNDLE, "fontSizes"), (FONT_SIZES, "large")]),
    })
}

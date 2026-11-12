//! Where the looks the loader draws with sit in the game's look bundle, read
//! by reflection name: label styles, sounds, setting texts and icons.

use crate::engine::loader::Loader;

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
    /// The bundle's icons, by [`Icon`], and the kind of the game's small
    /// standalone button that shows them; `None` keeps buttons to text.
    pub(super) icons: Option<([usize; 2], u32)>,
    /// The font size of a list entry's name, the bundle's `large`; `None`
    /// keeps names at their style's size.
    pub(super) name_size: Option<usize>,
}

/// An icon of the game's own look bundle that a button can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui::interface) enum Icon {
    /// The gear of the game's own settings buttons.
    Settings = 0,
    /// The circled i the game puts beside details.
    Info = 1,
}

/// The bundle offsets of the [`Icon`]s, from reflection, in their order,
/// with the small button's kind; `None`, logged once at start, keeps icon
/// buttons to their text.
fn icons(loader: &Loader) -> Option<([usize; 2], u32)> {
    let found = generic_icons(loader).zip(small_button(loader));
    if found.is_none() {
        crate::log!(
            WARN,
            "interface: icon buttons show their text on {}: the look bundle's icons or the small button's look do not read",
            loader.build.name
        );
    }
    found
}

const GENERIC_ICONS: &str = "keen::FbUiGenericIcons";
/// The bytes of a `UiTexture` the loader copies into a button's config.
const TEXTURE_SIZE: u32 = 16;

fn generic_icons(loader: &Loader) -> Option<[usize; 2]> {
    let generic = |name| {
        let info = loader.reflection.type_info(GENERIC_ICONS).ok()?;
        let texture = info
            .fields()
            .iter()
            .find(|f| f.name() == name)?
            .type_name()?;
        let size = loader.reflection.type_info(texture).ok()?.size();
        (size == TEXTURE_SIZE).then_some(())?;
        path(
            loader,
            &[
                (BUNDLE, "icons"),
                ("keen::FbUiIcons", "genericIcons"),
                (GENERIC_ICONS, name),
            ],
        )
    };
    Some([generic("settings")?, generic("infoCircle")?])
}

/// The kind `ui.button` takes for the game's small standalone button: the
/// index of `standaloneSmall` among the looks of `FbUiMenuButton`, which the
/// button's frame indexes by its kind (client 26430625).
fn small_button(loader: &Loader) -> Option<u32> {
    let info = loader.reflection.type_info("keen::FbUiMenuButton").ok()?;
    let fields: Vec<(&str, u64)> = info
        .fields()
        .iter()
        .map(|f| (f.name(), f.offset()))
        .collect();
    look_index(&fields, "standaloneSmall")
}

/// The index of look `name` among `fields`, `(name, offset)` in the
/// engine's order; `None` unless the looks sit one stride apart from the
/// first, as an array the kind indexes.
pub(super) fn look_index(fields: &[(&str, u64)], name: &str) -> Option<u32> {
    let first = fields.first()?.1;
    let stride = fields.get(1)?.1.checked_sub(first)?;
    let even = (0u64..).zip(fields).all(|(i, (_, offset))| {
        i.checked_mul(stride).and_then(|at| at.checked_add(first)) == Some(*offset)
    });
    if stride == 0 || !even {
        return None;
    }
    u32::try_from(fields.iter().position(|(field, _)| *field == name)?).ok()
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

#[cfg(test)]
mod tests {
    use super::look_index;

    #[test]
    fn a_look_s_kind_is_its_place_among_looks_one_stride_apart() {
        let looks = [
            ("standalone", 0),
            ("inGroupLeft", 0x40),
            ("inGroupMiddle", 0x80),
            ("inGroupRight", 0xc0),
            ("standaloneSmall", 0x100),
        ];
        assert_eq!(look_index(&looks, "standaloneSmall"), Some(4));
        assert_eq!(look_index(&looks, "standalone"), Some(0));
        assert_eq!(look_index(&looks, "huge"), None, "a look the build lacks");
        let mut uneven = looks;
        uneven[2].1 = 0x88;
        assert_eq!(
            look_index(&uneven, "standaloneSmall"),
            None,
            "a field that is no look"
        );
        assert_eq!(look_index(&looks[..1], "standalone"), None, "no stride");
        assert_eq!(look_index(&[("a", 0), ("b", 0)], "b"), None, "no stride");
    }
}

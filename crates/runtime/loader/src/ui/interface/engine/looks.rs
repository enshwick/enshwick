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
/// with the small button's kind; `None`, logged once at start with the
/// check that failed, keeps icon buttons to their text.
fn icons(loader: &Loader) -> Option<([usize; 2], u32)> {
    let found = generic_icons(loader)
        .and_then(|icons| Ok((icons, small_button(loader)?)))
        .inspect_err(|unread| {
            crate::log!(
                WARN,
                "interface: icon buttons show their text on {}: {unread}",
                loader.build.name
            );
        });
    found.ok()
}

const GENERIC_ICONS: &str = "keen::FbUiGenericIcons";
const MENU_BUTTON: &str = "keen::FbUiMenuButton";
const SMALL_LOOK: &str = "standaloneSmall";
/// The bytes of a `UiTexture` the loader copies into a button's config.
const TEXTURE_SIZE: u32 = 16;

/// Why the look bundle's icons or the small button's look do not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum IconsUnread {
    #[error("reflection: {0}")]
    Reflection(#[from] enshwick_gamedata::reflect::LookupError),
    #[error("{GENERIC_ICONS} has no typed field {0}")]
    Field(&'static str),
    #[error("{GENERIC_ICONS}.{name} is {size} bytes, no {TEXTURE_SIZE}-byte texture")]
    Size { name: &'static str, size: u32 },
    #[error("the look bundle's path to {GENERIC_ICONS}.{0} does not read")]
    Path(&'static str),
    #[error("{MENU_BUTTON}'s looks are no array of one stride holding {SMALL_LOOK}")]
    Looks,
}

fn generic_icons(loader: &Loader) -> Result<[usize; 2], IconsUnread> {
    let generic = |name: &'static str| -> Result<usize, IconsUnread> {
        let texture = loader
            .reflection
            .type_info(GENERIC_ICONS)?
            .fields()
            .iter()
            .find(|f| f.name() == name)
            .and_then(enshwick_gamedata::reflect::Field::type_name)
            .ok_or(IconsUnread::Field(name))?;
        let size = loader.reflection.type_info(texture)?.size();
        if size != TEXTURE_SIZE {
            return Err(IconsUnread::Size { name, size });
        }
        path(
            loader,
            &[
                (BUNDLE, "icons"),
                ("keen::FbUiIcons", "genericIcons"),
                (GENERIC_ICONS, name),
            ],
        )
        .ok_or(IconsUnread::Path(name))
    };
    Ok([generic("settings")?, generic("infoCircle")?])
}

/// The kind `ui.button` takes for the game's small standalone button: the
/// index of `standaloneSmall` among the looks of `FbUiMenuButton`, which the
/// button's frame indexes by its kind (client 26430625).
fn small_button(loader: &Loader) -> Result<u32, IconsUnread> {
    let info = loader.reflection.type_info(MENU_BUTTON)?;
    let fields: Vec<(&str, Option<&str>, u64)> = info
        .fields()
        .iter()
        .map(|f| (f.name(), f.type_name(), f.offset()))
        .collect();
    look_index(&fields, SMALL_LOOK).ok_or(IconsUnread::Looks)
}

/// The index of look `name` among `fields`, `(name, type, offset)` in the
/// engine's order: the looks are the leading fields of the first field's
/// type, which must sit one stride apart, as an array the kind indexes;
/// fields after them (on client 26430625 `sliderKnob`, `labelStyle` and
/// colours) are no looks.
pub(super) fn look_index(fields: &[(&str, Option<&str>, u64)], name: &str) -> Option<u32> {
    let (_, look, first) = *fields.first()?;
    look?;
    let looks = fields
        .iter()
        .take_while(|(_, ty, _)| *ty == look)
        .collect::<Vec<_>>();
    let stride = looks.get(1)?.2.checked_sub(first)?;
    let even = (0u64..).zip(&looks).all(|(i, (_, _, offset))| {
        i.checked_mul(stride).and_then(|at| at.checked_add(first)) == Some(*offset)
    });
    if stride == 0 || !even {
        return None;
    }
    u32::try_from(looks.iter().position(|(field, _, _)| *field == name)?).ok()
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
    use super::{GENERIC_ICONS, IconsUnread, look_index};

    #[test]
    fn an_unread_icon_names_the_check_that_failed() {
        let missing = IconsUnread::from(enshwick_gamedata::reflect::LookupError::Missing(
            GENERIC_ICONS.into(),
        ));
        assert_eq!(
            missing.to_string(),
            "reflection: keen::FbUiGenericIcons not found"
        );
        assert_eq!(
            IconsUnread::Field("settings").to_string(),
            "keen::FbUiGenericIcons has no typed field settings"
        );
        assert_eq!(
            IconsUnread::Size {
                name: "settings",
                size: 8
            }
            .to_string(),
            "keen::FbUiGenericIcons.settings is 8 bytes, no 16-byte texture"
        );
    }

    const LOOK: Option<&str> = Some("keen::FbUiMenuButtonTextures");

    /// `keen::FbUiMenuButton` as client 26430625 reflects it.
    const BUTTON: [(&str, Option<&str>, u64); 12] = [
        ("standalone", LOOK, 0),
        ("inGroupLeft", LOOK, 0x80),
        ("inGroupMiddle", LOOK, 0x100),
        ("inGroupRight", LOOK, 0x180),
        ("standaloneSmall", LOOK, 0x200),
        ("inGroupLeftSmall", LOOK, 0x280),
        ("inGroupRightSmall", LOOK, 0x300),
        ("sliderKnob", Some("keen::UiTexture"), 0x380),
        ("labelStyle", Some("keen::UiCharacterStyle"), 0x390),
        ("ornamentTint", Some("keen::UiColor"), 0x3e8),
        ("destructiveTint", Some("keen::UiColor"), 0x3f0),
        ("textColorDestructive", Some("keen::UiColor"), 0x3f8),
    ];

    #[test]
    fn a_look_s_kind_is_its_place_among_the_leading_looks_one_stride_apart() {
        assert_eq!(look_index(&BUTTON, "standaloneSmall"), Some(4));
        assert_eq!(look_index(&BUTTON, "standalone"), Some(0));
        assert_eq!(look_index(&BUTTON, "huge"), None, "a look the build lacks");
        assert_eq!(
            look_index(&BUTTON, "labelStyle"),
            None,
            "a field after the looks"
        );
        let mut uneven = BUTTON;
        uneven[2].2 = 0x108;
        assert_eq!(
            look_index(&uneven, "standaloneSmall"),
            None,
            "looks that are no array"
        );
        assert_eq!(look_index(&BUTTON[..1], "standalone"), None, "no stride");
        let same = [("a", LOOK, 0), ("b", LOOK, 0)];
        assert_eq!(look_index(&same, "b"), None, "no stride");
        let untyped = [("a", None, 0), ("b", None, 0x80)];
        assert_eq!(look_index(&untyped, "b"), None, "looks of no known type");
    }

    /// On the client `ENSH_CLIENT_EXE` names, the small button's look reads
    /// from the reflection as kind 4.
    #[test]
    #[ignore = "needs ENSH_CLIENT_EXE"]
    fn the_client_s_small_button_look_reads_from_its_reflection() {
        let path =
            std::env::var("ENSH_CLIENT_EXE").expect("ENSH_CLIENT_EXE names the game executable");
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let reflection = enshwick_gamedata::reflect::Reflection::scan(&image);
        let info = reflection.type_info(super::MENU_BUTTON).unwrap();
        let fields: Vec<_> = info
            .fields()
            .iter()
            .map(|f| (f.name(), f.type_name(), f.offset()))
            .collect();
        assert_eq!(look_index(&fields, super::SMALL_LOOK), Some(4));
    }
}

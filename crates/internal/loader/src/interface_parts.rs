//! The interface's own game functions and its optional parts, each with the
//! functions only it uses: a part whose function is missing or does not read
//! turns off alone, and the rest of the interface stays on.

use crate::pinned::{self, Symbol};

/// Why the interface stays off on this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Missing {
    Entry(&'static str),
    Bytes(&'static str),
    Room(&'static str),
    Looks,
    /// A type and its field the build's reflection lacks.
    Reflected(&'static str, &'static str),
}

impl Missing {
    /// The table entry or the reflected name that failed.
    pub(crate) const fn id(&self) -> &'static str {
        match self {
            Self::Entry(name) | Self::Bytes(name) | Self::Room(name) | Self::Reflected(_, name) => {
                name
            }
            Self::Looks => "the look bundle",
        }
    }

    /// Why it failed, after its [`Missing::id`].
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::Entry(_) => crate::builds::ABSENT.to_owned(),
            Self::Bytes(_) => "does not read as the build table says".to_owned(),
            Self::Room(_) => "runs past the loader's buffer".to_owned(),
            Self::Looks => "lacks a reflected field".to_owned(),
            Self::Reflected(ty, _) => format!("of {ty} is not reflected on this build"),
        }
    }
}

impl core::fmt::Display for Missing {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {}", self.id(), self.reason())
    }
}

/// An optional part of the interface and the game functions only it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Part {
    /// The part, as the log and the Mods screen name it.
    pub(crate) name: &'static str,
    /// The functions and hook sites only this part uses.
    pub(crate) functions: &'static [Symbol],
    /// Whether the part is a menu's Mods entry, which also needs the
    /// [`MENU_BACK`] functions the Mods screen hears Back with.
    pub(crate) menu: bool,
}

impl Part {
    pub(crate) const MAIN_MENU: Self = Self {
        name: "Mods entry in the main menu",
        functions: &[pinned::ui::MAIN_MENU_SITE, pinned::ui::MAIN_ENTRY],
        menu: true,
    };
    pub(crate) const PAUSE_MENU: Self = Self {
        name: "Mods entry in the pause menu",
        functions: &[pinned::ui::PAUSE_MENU_SITE, pinned::ui::PAUSE_ENTRY],
        menu: true,
    };
    pub(crate) const WORLD_ROWS: Self = Self {
        name: "Mods rows in the world settings",
        functions: &[pinned::ui::WORLD_SETTINGS_SITE],
        menu: false,
    };
    pub(crate) const HUD: Self = Self {
        name: "HUD",
        functions: &[pinned::ui::HUD, pinned::ui::HUD_VISIBLE],
        menu: false,
    };
    pub(crate) const ALL: [Self; 4] = [
        Self::MAIN_MENU,
        Self::PAUSE_MENU,
        Self::WORLD_ROWS,
        Self::HUD,
    ];
}

/// The functions both menus' Mods entries need: the Mods screen they open
/// hears Back through them.
pub(crate) const MENU_BACK: [Symbol; 2] = [pinned::ui::ACTION_NEW, pinned::ui::ACTION_QUERY];

/// Checks the interface's own functions with `check`, then each optional
/// part's: the first own function that fails keeps the interface off, and
/// each part with a function that fails comes back with why, to turn off
/// alone. A failed [`MENU_BACK`] function turns off both menus' entries.
///
/// # Errors
///
/// The first of [`FUNCTIONS`] that `check` refuses.
pub(crate) fn check_functions(
    check: impl Fn(Symbol) -> Result<(), Missing>,
) -> Result<Vec<(Part, Missing)>, Missing> {
    for name in FUNCTIONS {
        check(name)?;
    }
    let back = MENU_BACK.iter().find_map(|&name| check(name).err());
    Ok(Part::ALL
        .into_iter()
        .filter_map(|part| {
            let own = part.functions.iter().find_map(|&name| check(name).err());
            own.or(back.filter(|_| part.menu)).map(|why| (part, why))
        })
        .collect())
}

/// Every function the interface itself calls or hooks, by symbol; each
/// must read its `<symbol>.bytes` constant, or the interface stays off.
pub(crate) const FUNCTIONS: [Symbol; 34] = [
    pinned::ui::POPUP,
    pinned::ui::WINDOW_BEGIN,
    pinned::ui::WINDOW_END,
    pinned::ui::FRAME_FROM_NODE,
    pinned::ui::FRAME_OPEN,
    pinned::ui::FRAME_CLOSE,
    pinned::ui::FRAME_STRETCH,
    pinned::ui::FRAME_COLOR,
    pinned::ui::NODE_PADDING,
    pinned::ui::NODE_SPACING,
    pinned::ui::NODE_ALIGN,
    pinned::ui::NODE_WIDTH,
    pinned::ui::NODE_HEIGHT,
    pinned::ui::NODE_STRETCH,
    pinned::ui::PANEL,
    pinned::ui::NAV_INIT,
    pinned::ui::NAV_INPUT,
    pinned::ui::NAV_SET,
    pinned::ui::NAV_PUSH,
    pinned::ui::NAV_END,
    pinned::ui::NAV_DROP,
    pinned::ui::LABEL_NEW,
    pinned::ui::LABEL_DRAW,
    pinned::ui::BUTTON,
    pinned::ui::TOGGLE,
    pinned::ui::SLIDER_INIT,
    pinned::ui::SLIDER,
    pinned::ui::DROPDOWN_INIT,
    pinned::ui::DROPDOWN,
    pinned::ui::PROGRESS,
    pinned::ui::SOUND,
    pinned::ui::LOCA_ID,
    pinned::ui::LOCA_TEXT,
    pinned::ui::CONTEXT,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A function belongs to the interface itself, to the menus' Back or
    /// to one optional part, never to two, so a part's failure never
    /// reaches the rest.
    #[test]
    fn every_interface_function_belongs_to_one_place() {
        let mut seen: Vec<Symbol> = FUNCTIONS.iter().chain(&MENU_BACK).copied().collect();
        for part in Part::ALL {
            for &name in part.functions {
                assert!(!seen.contains(&name), "{name} in two places");
                seen.push(name);
            }
        }
        let mut unique = seen.clone();
        unique.sort_by_key(|s| s.id());
        unique.dedup();
        assert_eq!(unique.len(), seen.len());
    }

    fn broken(broken: Symbol) -> impl Fn(Symbol) -> Result<(), Missing> {
        move |name| {
            if name == broken {
                Err(Missing::Bytes(name.id()))
            } else {
                Ok(())
            }
        }
    }

    /// A function of an optional part that does not read turns only that
    /// part off, with its reason, and the interface stays on.
    #[test]
    fn a_broken_part_function_turns_only_that_part_off() {
        assert!(check_functions(|_| Ok(())).unwrap().is_empty());
        for part in Part::ALL {
            for &name in part.functions {
                let off = check_functions(broken(name)).unwrap();
                assert_eq!(off, [(part, Missing::Bytes(name.id()))], "{name}");
            }
        }
    }

    /// A broken Back turns off both menus' Mods entries and nothing else.
    #[test]
    fn a_broken_back_turns_off_both_menu_entries() {
        for name in MENU_BACK {
            let off: Vec<Part> = check_functions(broken(name))
                .unwrap()
                .into_iter()
                .map(|(part, _)| part)
                .collect();
            assert_eq!(off, [Part::MAIN_MENU, Part::PAUSE_MENU], "{name}");
        }
    }

    /// One of the interface's own functions that does not read keeps the
    /// whole interface off.
    #[test]
    fn a_broken_own_function_keeps_the_interface_off() {
        let own = FUNCTIONS[0];
        assert_eq!(check_functions(broken(own)), Err(Missing::Bytes(own.id())));
    }

    /// The reason a part is off reads as one line after its entry.
    #[test]
    fn a_missing_entry_names_itself_then_why() {
        let why = Missing::Bytes(pinned::ui::HUD.id());
        assert_eq!(why.id(), "ui.hud");
        assert_eq!(
            why.to_string(),
            "ui.hud does not read as the build table says"
        );
        let reflected = Missing::Reflected("keen::GameUiScrollbarParameterData", "sliderTexture");
        assert_eq!(
            reflected.to_string(),
            "sliderTexture of keen::GameUiScrollbarParameterData is not reflected on this build"
        );
    }
}

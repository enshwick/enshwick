//! The main menu's line under the game's version, and when it stays off.

/// What the main menu's line adds after the count of mods.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MenuNote {
    /// This game version leaves some of the loader's features off.
    Lacking,
    /// The loader settings file has problems.
    BadSettings,
    /// Developer tools are on.
    Developer,
}

impl MenuNote {
    /// The key of the text that adds the note to a line.
    const fn key(self) -> &'static str {
        match self {
            Self::Lacking => "menu.line.lacking",
            Self::BadSettings => "menu.line.settings",
            Self::Developer => "menu.line.developer",
        }
    }
}

/// The main menu's line under the game's version, in the reader's language.
///
/// It names the loader's version and how many `mods` load and how many of
/// them are `off`, then each of `notes` in [`MenuNote`]'s order.
#[must_use]
pub fn menu_line(version: &str, mods: usize, off: usize, notes: &[MenuNote]) -> String {
    let mods = mods.to_string();
    let off = off.to_string();
    let line = if off == "0" {
        crate::texts::fill("menu.line", &[("version", version), ("mods", &mods)])
    } else {
        crate::texts::fill(
            "menu.line.off",
            &[("version", version), ("mods", &mods), ("off", &off)],
        )
    };
    let mut notes = notes.to_vec();
    notes.sort_unstable();
    notes.dedup();
    notes.into_iter().fold(line, |line, note| {
        crate::texts::fill(note.key(), &[("line", &line)])
    })
}

/// Why the main menu's line stays off on this build; `None` when it shows.
///
/// The hook needs its site, which the build's table names (`in_table`) and
/// whose code holds there (`site`), and reflection must know the version
/// line's look (`look`).
#[must_use]
pub const fn menu_line_off(site: bool, in_table: bool, look: bool) -> Option<&'static str> {
    if !site {
        return Some(if in_table {
            "the code at ui.legal_line_site does not match its pattern or bytes"
        } else {
            "its table has no ui.legal_line_site"
        });
    }
    if !look {
        return Some("reflection lacks the version line's look (legalText, fontSizes.xsmall)");
    }
    None
}

/// The main menu's line, kept with the count of mods off it was made for,
/// so a frame only makes it again when that count changes.
#[derive(Debug, Default)]
pub struct KeptLine(Option<(usize, String)>);

impl KeptLine {
    /// No line yet.
    #[must_use]
    pub const fn new() -> Self {
        Self(None)
    }

    /// The line for `off` mods off, made by `make` the first time and
    /// whenever `off` changed since.
    pub fn get(&mut self, off: usize, make: impl FnOnce() -> String) -> &str {
        if self.0.as_ref().is_none_or(|(was, _)| *was != off) {
            self.0 = Some((off, make()));
        }
        self.0.as_ref().map_or("", |(_, line)| line.as_str())
    }
}

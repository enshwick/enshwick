//! The game's HUD screens by name: each member of the reflected
//! `HudScreenType` by the screen byte it stands for, for every layer that
//! names a screen.

use std::sync::OnceLock;

use enshwick_gamedata::catalogue;

use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// The name of each screen byte; empty for a byte no member names.
static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
/// The interface's screen, prompt and host reads, off on a build that lacks
/// them and at their first panic; the frames then read as outside a game,
/// and the client snapshot answers off.
pub(crate) static READS_FUSE: Fuse = Fuse::new("screen reads");

/// Each screen byte's name on `loader`'s build, or the type's name when the
/// build does not reflect it.
fn names(loader: &Loader) -> Result<Vec<&'static str>, &'static str> {
    let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
    let info = loader.reflection.type_info(screens).map_err(|_| screens)?;
    let mut names = vec![""; usize::from(u8::MAX) + 1];
    for constant in info.enum_values() {
        if let Some(slot) = usize::try_from(constant.value())
            .ok()
            .and_then(|value| names.get_mut(value))
        {
            *slot = constant.name();
        }
    }
    Ok(names)
}

/// Keeps the screens' names of `loader`'s build.
///
/// # Errors
///
/// The reflected type's name when the build does not reflect it: every
/// screen is then unnamed.
pub(crate) fn link(loader: &Loader) -> Result<(), &'static str> {
    let _first = NAMES.set(names(loader)?);
    Ok(())
}

/// The game's name of screen byte `screen`; empty for a byte no member
/// names, and before [`link`].
pub(crate) fn name(screen: u8) -> &'static str {
    NAMES
        .get()
        .and_then(|names| names.get(usize::from(screen)).copied())
        .unwrap_or_default()
}

/// The screen byte the game names `name`; `None` for an empty or unknown
/// name, and before [`link`].
pub(crate) fn byte(name: &str) -> Option<u8> {
    let names = NAMES.get()?;
    let at = names
        .iter()
        .position(|named| !name.is_empty() && *named == name)?;
    u8::try_from(at).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_the_screens_reflected_names_none() {
        let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
        assert_eq!(names(&crate::start::tests::bare_loader()), Err(screens));
        assert_eq!(link(&crate::start::tests::bare_loader()), Err(screens));
        assert_eq!(name(1), "");
        assert_eq!(byte("InGame"), None);
        assert_eq!(byte(""), None);
    }
}

//! Linking the loader's join check and its dialog hold on the client.

use crate::host::{Loader, Target};
use crate::pinned;

use super::game::identify;

/// The build table ids this part of the loader names.
#[cfg(windows)]
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::net::JOIN_GAME.id(),
    pinned::net::PROVIDER_VTABLE.id(),
    pinned::user::START_INTERACTION.id(),
    pinned::user::INTERACTION_KIND.id(),
    pinned::user::JOIN_FAILED_KIND.id(),
];

/// Links the loader's join check on the client's join. The lobby module and
/// the mod names serve hosted games too, so they go in on every build.
pub(super) fn link_join(loader: &Loader) {
    let names = loader
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.name()))
        .filter(|(_, name)| !name.is_empty())
        .collect();
    crate::join::set_names(names);
    crate::lobby::register();
    match verified_join(loader) {
        Ok((join, code)) => {
            let linked = loader.link_feature(
                "join check",
                Target::Found(pinned::net::JOIN_GAME.id(), Some(join)),
                &crate::join::NEXT,
                crate::join::detour,
                enshwick::abi::HookFlags::NONE,
            );
            if linked {
                link_join_hold(loader, code);
            }
        }
        Err(error) => crate::log!(
            WARN,
            "join check off on {}: {error}; nothing patched",
            loader.build.name
        ),
    }
}

/// Links the hold on the game's own dialog for a refused join, once the start
/// of user interactions lies in `code`, the game's code section, and starts
/// with the pinned bytes. Without it a refused join shows the game's dialog
/// before the loader's.
fn link_join_hold(loader: &Loader, code: core::ops::Range<u64>) {
    use crate::join::interaction::{self, BYTES_ID};
    let read = |va: u64| {
        let address = usize::try_from(va).ok()?;
        // SAFETY: `verify` reads only eight bytes inside the game's code
        // section, which stays mapped for the life of the process.
        Some(unsafe { crate::memory::read::<u64>(address, 0) })
    };
    let start = interaction::verify(
        loader.address(pinned::user::START_INTERACTION),
        loader.build.constant(BYTES_ID),
        read,
        code,
    );
    let start = match start {
        Ok(start) => start,
        Err(error) => {
            crate::log!(
                WARN,
                "join dialog hold off on {}: {error}; nothing patched",
                loader.build.name
            );
            return;
        }
    };
    if !interaction::set_layout(
        loader.build.constant(pinned::user::INTERACTION_KIND.id()),
        loader.build.constant(pinned::user::JOIN_FAILED_KIND.id()),
    ) {
        crate::log!(
            WARN,
            "join dialog hold off on {}: the build table pins no {} or {}; nothing patched",
            loader.build.name,
            pinned::user::INTERACTION_KIND,
            pinned::user::JOIN_FAILED_KIND,
        );
        return;
    }
    loader.link_feature(
        "join dialog hold",
        Target::Found(pinned::user::START_INTERACTION.id(), Some(start)),
        &interaction::NEXT,
        interaction::detour,
        enshwick::abi::HookFlags::NONE,
    );
}

/// The online provider's join, once the provider's function table holds it
/// in exactly one slot and its code starts with the pinned bytes, and the
/// game's code section.
fn verified_join(
    loader: &Loader,
) -> Result<(u64, core::ops::Range<u64>), crate::join::provider::Error> {
    use crate::join::provider::{Error, Pins, verify_mapped};
    let pins = Pins::from_table(
        |id| loader.build.symbol(id),
        |id| loader.build.constant(id),
        loader.base,
    )?;
    let range = |start: u64, len: usize| Some(start..start.checked_add(u64::try_from(len).ok()?)?);
    let (_, base, image_len) = identify().ok_or(Error::NoImage)?;
    let image = range(base, image_len).ok_or(Error::NoImage)?;
    let code = crate::code::text(base)
        .and_then(|(start, len)| range(u64::try_from(start).ok()?, len))
        .ok_or(Error::NoCode)?;
    // SAFETY: `image` is the game's executable image, mapped by Windows at
    // `base` for the life of the process, and the code words it reads are
    // never written.
    let join = unsafe { verify_mapped(&pins, image, code.clone()) }?;
    Ok((join, code))
}

/// The homepages of the running join mods that declare one, `(id, page)`.
pub(super) fn pages(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        .filter(|m| m.joins() && !m.meta.homepage.is_empty())
        .map(|m| (m.id.clone(), m.meta.homepage.clone()))
        .collect()
}

/// The names of the running join mods that declare one, `(id, name)`.
pub(super) fn names(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        // The manifest's name: players of any language read the list.
        .filter(|m| m.joins() && !m.meta.name.is_empty())
        .map(|m| (m.id.clone(), m.meta.name.clone()))
        .collect()
}

//! Linking the loader's join check and its dialog hold on the client.

use crate::base::join::JOIN_FUSE;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::pinned;

use super::game::identify;

/// The build table ids this part of the loader names.
#[cfg(windows)]
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::join::TARGET_COPY.id(),
    pinned::join::TARGET_GAME_PORT_READ.id(),
    pinned::join::TARGET_HAS_ADDRESS.id(),
    pinned::join::TARGET_HOST.id(),
    pinned::join::TARGET_IP_READ.id(),
    pinned::join::TARGET_PASSWORD.id(),
    pinned::join::TARGET_QUERY_PORT_READ.id(),
    pinned::join::TARGET_WORD_COPY.id(),
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
    crate::base::join::set_names(names);
    crate::base::lobby::register();
    link_check(loader);
}

/// Links the join check once the build's join target layout reads and the
/// provider's join checks out; otherwise turns it off, naming why.
fn link_check(loader: &Loader) {
    let join_off = |why: &dyn core::fmt::Display| {
        not_checked(loader, &JOIN_FUSE, pinned::net::JOIN_GAME.id(), why);
    };
    match crate::base::join::Layout::from_table(|id| loader.build.constant(id)) {
        Ok(layout) => crate::base::join::set_layout(layout),
        Err(error) => return join_off(&error),
    }
    match verified_join(loader) {
        Ok((join, code)) => {
            if crate::base::join::link(loader, join) {
                link_join_hold(loader, code);
            }
        }
        Err(error) => join_off(&error),
    }
}

/// Links the hold on the game's own dialog for a refused join, once the start
/// of user interactions lies in `code`, the game's code section, and starts
/// with the pinned bytes. Without it a refused join shows the game's dialog
/// before the loader's.
fn link_join_hold(loader: &Loader, code: core::ops::Range<u64>) {
    use crate::base::join::interaction::{self, BYTES_ID, HOLD_FUSE};
    let read = |va: u64| {
        let address = usize::try_from(va).ok()?;
        // SAFETY: `verify` reads only eight bytes inside the game's code
        // section, which stays mapped for the life of the process.
        Some(unsafe { crate::engine::memory::read::<u64>(address, 0) })
    };
    let start = interaction::verify(
        loader.address(pinned::user::START_INTERACTION),
        loader.build.constant(BYTES_ID),
        loader.build.routed(pinned::user::START_INTERACTION.id()),
        read,
        code,
    );
    let hold_off = |why: &dyn core::fmt::Display| {
        not_checked(
            loader,
            &HOLD_FUSE,
            pinned::user::START_INTERACTION.id(),
            why,
        );
    };
    let start = match start {
        Ok(start) => start,
        Err(error) => return hold_off(&error),
    };
    if !interaction::set_layout(
        loader.build.constant(pinned::user::INTERACTION_KIND.id()),
        loader.build.constant(pinned::user::JOIN_FAILED_KIND.id()),
    ) {
        return hold_off(&format_args!(
            "this build lacks {} or {}",
            pinned::user::INTERACTION_KIND,
            pinned::user::JOIN_FAILED_KIND,
        ));
    }
    let _linked = interaction::link(loader, start);
}

/// Turns `fuse` off, its entry `id` not checked because `why`.
fn not_checked(loader: &Loader, fuse: &'static Fuse, id: &str, why: &dyn core::fmt::Display) {
    let _off = loader.fuse_off(fuse, id, &format!("is not checked: {why}"));
}

/// The online provider's join, once the provider's function table holds it
/// in exactly one slot and its code starts with the pinned bytes, and the
/// game's code section.
fn verified_join(
    loader: &Loader,
) -> Result<(u64, core::ops::Range<u64>), crate::base::join::provider::Error> {
    use crate::base::join::provider::{Error, Pins, verify_mapped};
    let pins = Pins {
        routed: loader.build.routed(pinned::net::JOIN_GAME.id()),
        ..Pins::from_table(
            |id| loader.build.symbol(id),
            |id| loader.build.constant(id),
            loader.base,
        )?
    };
    let range = |start: u64, len: usize| Some(start..start.checked_add(u64::try_from(len).ok()?)?);
    let (_, base, image_len) = identify().ok_or(Error::NoImage)?;
    let image = range(base, image_len).ok_or(Error::NoImage)?;
    let code = crate::engine::code::text(base)
        .and_then(|(start, len)| range(u64::try_from(start).ok()?, len))
        .ok_or(Error::NoCode)?;
    // SAFETY: `image` is the game's executable image, mapped by Windows at
    // `base` for the life of the process, and the code words it reads are
    // never written.
    let join = unsafe { verify_mapped(&pins, image, code.clone()) }?;
    Ok((join, code))
}

/// The names of the running mods of side both that declare one, `(id, name)`.
pub(super) fn names(loader: &Loader) -> Vec<(String, String)> {
    loader
        .mods
        .iter()
        // The manifest's name: players of any language read the list.
        .filter(|m| m.joins() && !m.meta.name.is_empty())
        .map(|m| (m.id.clone(), m.meta.name.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test for both: the join check's fuse is one per process, and
    // `cargo test` runs every test in one.
    #[test]
    fn the_join_links_and_once_off_it_lists_why_and_holds_back_no_game_dialog() {
        use crate::base::join::interaction;
        use crate::engine::mod_index::ModIndex;
        use enshwick_base::Relock;
        const JOIN_FAILED: u8 = 7;
        unsafe extern "system" fn game(_: *mut u8, _: *const u8) -> u32 {
            9
        }
        let loader = crate::start::tests::bare_loader();
        assert!(crate::base::join::link(&loader, 0x7100));
        assert!(interaction::link(&loader, 0x7200));
        {
            let chains = loader.chains.relock();
            assert_eq!(chains.owners(0x7100), [ModIndex::LOADER]);
            assert_eq!(chains.owners(0x7200), [ModIndex::LOADER]);
            let fuses: Vec<_> = [0x7100, 0x7200]
                .iter()
                .flat_map(|&at| chains.features_at(at))
                .map(Fuse::feature)
                .collect();
            assert_eq!(
                fuses,
                [JOIN_FUSE.feature(), interaction::HOLD_FUSE.feature()]
            );
        }
        link_check(&crate::start::tests::bare_loader());
        assert!(JOIN_FUSE.is_off());
        let reason = crate::engine::diag::off_reason(&JOIN_FUSE).unwrap_or_default();
        assert!(
            reason.starts_with(pinned::net::JOIN_GAME.id())
                && reason.contains(pinned::join::TARGET_PASSWORD.id()),
            "{reason}"
        );
        // A refusal armed the hold, then the check turned off (a panic does
        // so mid-join): the join it let through keeps the game's dialog.
        assert!(interaction::set_layout(Some(0), Some(JOIN_FAILED.into())));
        interaction::joining(true);
        let request = [JOIN_FAILED];
        assert_eq!(interaction::start_through_hold(request.as_ptr(), game), 9);
        interaction::joining(false);
    }
}

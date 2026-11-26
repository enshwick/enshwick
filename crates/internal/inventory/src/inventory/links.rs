//! The one walk of an `InventorySetup`'s links, which the API's reads and
//! every reader of the loader share.

use core::ops::ControlFlow;

use super::LINKS;
use super::slots::InventoryCategory;
use crate::Entity;
use crate::raw::read_at;

/// Where an `InventorySetup`'s fields sit on a build, from its reflection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "hidden: only the loader and the fake host name it"
)]
pub struct SetupLayout {
    /// Its `linksEntities`, [`LINKS`] `u32`.
    pub links: usize,
    /// Its `linksCategories`, [`LINKS`] bytes.
    pub categories: usize,
    /// Its `isInitialized` byte.
    pub initialized: usize,
}

/// Hands `visit` every link of `root`'s `InventorySetup` it does not walk
/// into, in the game's order, for every reader of an inventory's slots.
///
/// It hands each link that is not virtual, and each virtual one found
/// `max_depth` links deep, whose own links it leaves unread. It follows the
/// other virtual links, passes over empty links, and leaves out the entity
/// `skip` (0 for none). A setup the game has not set up holds nothing.
/// `setup` finds an entity's setup; the walk ends at the first
/// [`ControlFlow::Break`] of `visit`.
///
/// # Safety
///
/// Every pointer `setup` returns is a live `InventorySetup` laid out as
/// `layout` says, readable for its [`LINKS`] links and categories and its
/// initialised byte, with no concurrent writer until the walk ends.
pub unsafe fn walk<B>(
    root: Entity,
    max_depth: u32,
    skip: Entity,
    layout: &SetupLayout,
    setup: &mut dyn FnMut(Entity) -> Option<*const u8>,
    visit: &mut dyn FnMut(Entity, InventoryCategory) -> ControlFlow<B>,
) -> ControlFlow<B> {
    // SAFETY: every pointer `setup` returns is a live `InventorySetup` of
    // `layout`, unwritten during the walk (this function's `# Safety`), the
    // same guarantee `walk_from` asks.
    unsafe { walk_from(root, 0, max_depth, skip, layout, setup, visit) }
}

/// [`walk`] from `root`, found `depth` links below the walk's root.
///
/// # Safety
///
/// Every pointer `setup` returns is a live `InventorySetup` laid out as
/// `layout` says, readable for its [`LINKS`] links and categories and its
/// initialised byte, with no concurrent writer until the walk ends.
unsafe fn walk_from<B>(
    root: Entity,
    depth: u32,
    max_depth: u32,
    skip: Entity,
    layout: &SetupLayout,
    setup: &mut dyn FnMut(Entity) -> Option<*const u8>,
    visit: &mut dyn FnMut(Entity, InventoryCategory) -> ControlFlow<B>,
) -> ControlFlow<B> {
    let Some(found) = setup(root) else {
        return ControlFlow::Continue(());
    };
    // SAFETY: `found` is a live `InventorySetup` (the caller's contract),
    // whose `isInitialized` byte sits at `layout.initialized`; read as a
    // `u8`, never a `bool`.
    if unsafe { read_at::<u8>(found, layout.initialized) } == 0 {
        return ControlFlow::Continue(());
    }
    for i in 0..LINKS {
        // SAFETY: `found` is a live `InventorySetup` (the caller's contract),
        // whose `LINKS` `u32` links start at `layout.links`, and `i < LINKS`.
        let link = Entity::from_engine(unsafe {
            read_at::<u32>(found, layout.links + i * size_of::<u32>())
        });
        // The game appends links packed, but its own setup check reads past
        // an empty one (`sys.inventory_setup`, server 26430253), so a gap
        // never hides the links after it.
        if link.is_none() || link == skip {
            continue;
        }
        // SAFETY: the same setup, whose `LINKS` category bytes start at
        // `layout.categories`, and `i < LINKS`.
        let category = InventoryCategory(unsafe { read_at::<u8>(found, layout.categories + i) });
        if category == InventoryCategory::VIRTUAL && depth < max_depth {
            // SAFETY: the same `setup` and `layout`, whose pointers are live
            // `InventorySetup`s unwritten during the walk (this function's
            // `# Safety`).
            unsafe { walk_from(link, depth + 1, max_depth, skip, layout, setup, visit) }?;
        } else {
            visit(link, category)?;
        }
    }
    ControlFlow::Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: SetupLayout = SetupLayout {
        links: 0,
        categories: 4 * LINKS,
        initialized: 5 * LINKS,
    };
    const VIRTUAL: u8 = InventoryCategory::VIRTUAL.0;
    const GENERIC: u8 = InventoryCategory::GENERIC.0;

    /// The bytes of a set-up `InventorySetup` with `links`, the rest empty.
    fn setup(links: &[(u32, u8)]) -> [u8; 6 * LINKS] {
        let mut bytes = [0u8; 6 * LINKS];
        for (i, &(link, category)) in links.iter().enumerate() {
            bytes[4 * i..4 * i + 4].copy_from_slice(&link.to_ne_bytes());
            bytes[4 * LINKS + i] = category;
        }
        bytes[5 * LINKS] = 1;
        bytes
    }

    /// Every link `walk` hands from `root` over `setups`, by entity.
    fn walked(
        setups: &[(u32, [u8; 6 * LINKS])],
        root: u32,
        max_depth: u32,
        skip: u32,
    ) -> Vec<(u32, u8)> {
        let mut out = Vec::new();
        let mut find = |entity: Entity| {
            setups
                .iter()
                .find(|(at, _)| *at == entity.get())
                .map(|(_, bytes)| bytes.as_ptr())
        };
        // SAFETY: every pointer `find` returns is a whole setup of `LAYOUT`,
        // kept by `setups` and not written during the walk.
        let ended = unsafe {
            walk::<()>(
                Entity::from_engine(root),
                max_depth,
                Entity::from_engine(skip),
                &LAYOUT,
                &mut find,
                &mut |link, category| {
                    out.push((link.get(), category.0));
                    ControlFlow::Continue(())
                },
            )
        };
        assert_eq!(ended, ControlFlow::Continue(()));
        out
    }

    #[test]
    fn virtual_links_are_followed_in_place_and_others_handed_in_order() {
        let setups = [
            (1, setup(&[(10, GENERIC), (2, VIRTUAL), (11, GENERIC)])),
            (2, setup(&[(20, InventoryCategory::EQUIPMENT.0)])),
        ];
        assert_eq!(
            walked(&setups, 1, 4, 0),
            [(10, GENERIC), (20, 2), (11, GENERIC)]
        );
    }

    #[test]
    fn a_virtual_link_at_the_depth_is_handed_unread() {
        let chain = [
            (1, setup(&[(2, VIRTUAL)])),
            (2, setup(&[(3, VIRTUAL)])),
            (3, setup(&[(30, GENERIC)])),
        ];
        assert_eq!(walked(&chain, 1, 1, 0), [(3, VIRTUAL)]);
        assert_eq!(walked(&chain, 1, 2, 0), [(30, GENERIC)]);
        assert_eq!(walked(&chain, 1, 0, 0), [(2, VIRTUAL)]);
    }

    #[test]
    fn the_walk_passes_over_empty_links_and_leaves_out_the_skipped_one() {
        let setups = [(1, setup(&[(10, GENERIC), (99, VIRTUAL), (11, GENERIC)]))];
        assert_eq!(walked(&setups, 1, 4, 99), [(10, GENERIC), (11, GENERIC)]);
        let gap = [(1, setup(&[(10, GENERIC), (0, GENERIC), (11, GENERIC)]))];
        assert_eq!(walked(&gap, 1, 4, 0), [(10, GENERIC), (11, GENERIC)]);
    }

    #[test]
    fn a_setup_the_game_has_not_set_up_or_none_hands_nothing() {
        let mut unset = setup(&[(10, GENERIC)]);
        unset[5 * LINKS] = 0;
        assert!(walked(&[(1, unset)], 1, 4, 0).is_empty());
        assert!(walked(&[], 1, 4, 0).is_empty());
    }

    #[test]
    fn a_break_of_the_visit_ends_the_whole_walk() {
        let setups = [
            (1, setup(&[(2, VIRTUAL), (11, GENERIC)])),
            (2, setup(&[(20, GENERIC), (21, GENERIC)])),
        ];
        let mut seen = Vec::new();
        let mut find = |entity: Entity| {
            setups
                .iter()
                .find(|(at, _)| *at == entity.get())
                .map(|(_, bytes)| bytes.as_ptr())
        };
        // SAFETY: as in `walked`.
        let ended = unsafe {
            walk(
                Entity::from_engine(1),
                4,
                Entity::NONE,
                &LAYOUT,
                &mut find,
                &mut |link, _| {
                    seen.push(link.get());
                    if link.get() == 20 {
                        ControlFlow::Break(link)
                    } else {
                        ControlFlow::Continue(())
                    }
                },
            )
        };
        assert_eq!(ended, ControlFlow::Break(Entity::from_engine(20)));
        assert_eq!(seen, [20]);
    }
}

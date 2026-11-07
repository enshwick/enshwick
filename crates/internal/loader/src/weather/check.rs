//! The live check the parts that write the weather run before their first
//! write on a world.

use enshwick::abi::Status;

use super::Layout;
use crate::host::Loader;
use crate::live::{self, Broken};
use crate::pinned;

/// More layers than phenomena a `u8` index can name: a count past it is a
/// misread.
const MAX_LAYERS: u64 = 256;
/// More instance pool pages than the 16-bit slot of a handle can reach: a
/// count past it is a misread.
const MAX_PAGES: u64 = 1 << 16;

/// The switch of [`super::set`] and [`super::remap`], the parts that write
/// the weather, off once the live system does not hold its found offsets.
static CHANGES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("weather changes");

/// Walks the live weather system of the running `server` the way
/// [`super::set`], [`super::empty`] and [`super::instance`] do, through reads
/// that cannot fault: the system and its layer array read, the layer and
/// page counts are ones a world keeps, the last layer slot reads whole and
/// the page shift fits a word.
pub(super) fn system_holds(layout: &Layout, server: usize) -> Result<(), Broken> {
    let system = live::pointer(pinned::weather::SYSTEM.id(), server, layout.system)?;
    let layers = live::count(
        pinned::weather::LAYER_COUNT.id(),
        system,
        layout.layer_count,
        MAX_LAYERS,
    )?;
    if let Some(last) = layers.checked_sub(1) {
        let array = live::pointer(pinned::weather::LAYERS.id(), system, layout.layers)?;
        let slot = array.wrapping_add(usize::try_from(last).unwrap_or(0) * layout.slot_size);
        live::bytes::<8>(pinned::weather::SLOT_TIME.id(), slot, layout.slot_time)?;
        live::bytes::<8>(
            pinned::weather::SLOT_PHENOMENON.id(),
            slot,
            layout.slot_phenomenon,
        )?;
        live::bytes::<4>(
            pinned::weather::SLOT_CURRENT.id(),
            slot,
            layout.slot_current,
        )?;
        live::bytes::<4>(pinned::weather::SLOT_NEXT.id(), slot, layout.slot_next)?;
    }
    let pages = live::count(
        pinned::weather::PAGE_COUNT.id(),
        system,
        layout.page_count,
        MAX_PAGES,
    )?;
    if pages > 0 {
        live::pointer(pinned::weather::PAGES.id(), system, layout.pages)?;
    }
    let [shift] = live::bytes::<1>(pinned::weather::PAGE_SHIFT.id(), system, layout.shift)?;
    live::holds(
        pinned::weather::PAGE_SHIFT.id(),
        shift < 64,
        "a shift under 64",
    )
}

/// Whether the weather may be written on this world: [`system_holds`] held
/// once on the live system.
pub(super) fn writable(loader: &Loader, layout: &Layout) -> Result<(), Status> {
    let server = crate::world::server()?;
    if CHANGES_FUSE.verify(&loader.build.name, || system_holds(layout, server)) {
        Ok(())
    } else {
        Err(Status::MISSING)
    }
}

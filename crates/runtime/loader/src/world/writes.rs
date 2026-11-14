//! Component writes for mods on the server's game thread, between steps or
//! inside a game event, where no ECS system runs.
//!
//! A write goes through the engine's own write accessor, which hands out the
//! component and marks it changed in the entity world's change tracker, so
//! replication sends it to clients and the save writes it; the game's own
//! game-thread code writes water blocks the same way. Copies the game keeps
//! for clients, the `Network*` and `Client*` components, are refused: the
//! server's systems overwrite them from their source every step.

use core::ffi::c_void;
use std::sync::OnceLock;

use rustc_hash::FxHashSet;

use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::CHANGE_TRACKER.id(),
    pinned::ecs::COMPONENT_WRITE.id(),
];

/// Name prefixes of the components the game derives for clients.
const DERIVED: [&str; 2] = ["keen::ecs::Network", "keen::ecs::Client"];

type WriteFn = unsafe extern "system" fn(
    out: *mut [usize; 2],
    entity_world: *mut u8,
    tracker: *mut u8,
    index: *const u16,
    entity: u32,
    flags: u8,
) -> *mut [usize; 2];

/// The write accessor and the change tracker's place, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    write: WriteFn,
    tracker: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
static REFUSED: OnceLock<FxHashSet<u32>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let write = usize::try_from(loader.address(pinned::ecs::COMPONENT_WRITE)?).ok()?;
            Some(Layout {
                // SAFETY: the symbol is the game's write accessor with this
                // signature on this build.
                write: unsafe { core::mem::transmute::<usize, WriteFn>(write) },
                tracker: loader.offset(pinned::ecs::CHANGE_TRACKER)?,
            })
        })
        .as_ref()
}

/// The type hashes of the components the game derives for clients.
fn refused(loader: &Loader) -> &'static FxHashSet<u32> {
    REFUSED.get_or_init(|| {
        loader
            .reflection
            .type_names()
            .filter(|name| DERIVED.iter().any(|prefix| name.starts_with(prefix)))
            .filter_map(|name| loader.reflection.type_info(name).ok())
            .filter_map(|info| crate::engine::world::type_hash(info.va()))
            .collect()
    })
}

/// Why a component was not handed out to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum WriteError {
    /// The build lacks the component write.
    #[error("this build lacks the component write")]
    Unavailable,
    /// Entity 0, which names none.
    #[error("no entity")]
    NoEntity,
    /// A type the loader keeps mods from writing.
    #[error("the type is not written by mods")]
    Refused,
    /// The static half of an ECS type, the template's set-up data.
    #[error("the type is a static half")]
    StaticHalf,
    /// The entity has no component of the type.
    #[error("the entity has no such component")]
    Absent,
    /// The server's world, unreadable now.
    #[error(transparent)]
    World(#[from] crate::engine::world::WorldError),
}

/// A writable pointer to `entity`'s component of type `hash`, marked changed.
pub(crate) fn component_mut(
    loader: &Loader,
    entity: u32,
    hash: u32,
) -> Result<*mut c_void, WriteError> {
    let layout = layout(loader).ok_or(WriteError::Unavailable)?;
    if entity == 0 {
        return Err(WriteError::NoEntity);
    }
    crate::engine::world::entity_call(loader)?;
    let at = crate::engine::world::type_at(loader, hash)?;
    if refused(loader).contains(&hash) {
        return Err(WriteError::Refused);
    }
    if crate::engine::world::names_static(loader, &at, hash) {
        return Err(WriteError::StaticHalf);
    }
    let (world, index) = (at.world, at.index);
    let mut out = [0usize; 2];
    // SAFETY: the server's entity world on its game thread between steps,
    // where no system runs; the tracker lies inside the entity world, and
    // `index` points into its registry.
    unsafe {
        (layout.write)(
            &raw mut out,
            core::ptr::with_exposed_provenance_mut(world),
            core::ptr::with_exposed_provenance_mut(world + layout.tracker),
            index,
            entity,
            0,
        );
    };
    match out[0] {
        0 => Err(WriteError::Absent),
        data => Ok(core::ptr::with_exposed_provenance_mut(data)),
    }
}

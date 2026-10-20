//! Entity labels on the server: the name a player gives a chest, which the
//! game shows on hover, sends to every client and saves with the world.
//!
//! A label is the entity's `CustomPlayerString`, an id into the server's
//! table of strings. Setting one repeats the game's own rename: find the
//! component, swap the string (the table keeps reference counts), then write
//! the id or add the component. The game checks no permission on a player's
//! rename, so the loader links the string swap and asks the mods' rename
//! filters first when the call comes from the rename message.

use core::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_api::abi::{Header, HookFlags, PlayerInfo, RenameCall, RenameFn, Status, Str};

use crate::chains::{LOADER, Link};
use crate::host::Loader;

/// The game's error for a string it could not take; the rename path then
/// changes nothing.
const REFUSED: u8 = 0x1f;
/// The reflected component that holds a label.
const LABEL_TYPE: &str = "keen::ecs::CustomPlayerString";

type FindFn = unsafe extern "system" fn(
    out: *mut [usize; 4],
    world: *mut u8,
    ty: u32,
    entity: u32,
) -> *mut usize;
type ReplaceFn = unsafe extern "system" fn(
    out: *mut [u64; 2],
    strings: *mut u8,
    old: u64,
    text: *const [usize; 2],
) -> *mut u8;
type AddFn = unsafe extern "system" fn(
    world: *mut u8,
    entity: u32,
    ty: u32,
    data: *const [usize; 2],
) -> usize;

/// The game functions and offsets labels use, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    find: FindFn,
    replace: ReplaceFn,
    add: AddFn,
    label_type: u32,
    world: usize,
    strings: usize,
    max_text: usize,
    rename_return: u64,
    rename_entity: usize,
}

/// A mod's rename filter.
#[derive(Debug, Clone, Copy)]
struct Filter {
    owner: usize,
    callback: RenameFn,
    user: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
static LOADED: OnceLock<&'static Loader> = OnceLock::new();
static REPLACE_NEXT: AtomicUsize = AtomicUsize::new(0);
static FILTERS: Mutex<Vec<Filter>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Box<[Filter]>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let symbol = |name: &str| {
                loader
                    .build
                    .symbol(name)
                    .and_then(|rva| usize::try_from(loader.base + rva).ok())
            };
            let constant = |name: &str| {
                loader
                    .build
                    .constant(name)
                    .and_then(|v| usize::try_from(v).ok())
            };
            let (find, replace, add) = (
                symbol("label.find")?,
                symbol("label.replace")?,
                symbol("label.add")?,
            );
            let descriptor = loader.reflection.type_info(LABEL_TYPE).ok()?.va();
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                find: unsafe { core::mem::transmute::<usize, FindFn>(find) },
                // SAFETY: as above.
                replace: unsafe { core::mem::transmute::<usize, ReplaceFn>(replace) },
                // SAFETY: as above.
                add: unsafe { core::mem::transmute::<usize, AddFn>(add) },
                label_type: crate::world::type_hash(loader, descriptor)?,
                world: constant("server.world")?,
                strings: constant("server.custom_strings")?,
                max_text: constant("label.max_text")?,
                rename_return: loader.base + loader.build.symbol("label.rename_return")?,
                rename_entity: constant("label.rename_entity")?,
            })
        })
        .as_ref()
}

/// Registers mod `owner`'s rename filter.
pub(crate) fn register(owner: usize, callback: RenameFn, user: *mut c_void) -> Status {
    if SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Filter {
            owner,
            callback,
            user: user.expose_provenance(),
        });
    Status::OK
}

/// Links the rename check when a mod filters renames.
pub(crate) fn link(loader: &'static Loader) {
    let filters = std::mem::take(&mut *FILTERS.lock().unwrap_or_else(PoisonError::into_inner));
    let wanted = !filters.is_empty();
    let _first = SEALED.set(filters.into_boxed_slice());
    let _first = LOADED.set(loader);
    if !wanted {
        return;
    }
    let Some(layout) = layout(loader) else {
        crate::log!(WARN, "rename filters off: this build has no label layout");
        return;
    };
    let link = Link {
        owner: LOADER,
        detour: (replace as ReplaceFn as *const ()).addr(),
        next: &REPLACE_NEXT,
        priority: i32::MIN,
        flags: HookFlags::CALLER_FRAME,
    };
    let target = (layout.replace as *const ()).addr() as u64;
    if let Ok(mut chains) = loader.chains.lock() {
        let status = chains.add(target, link);
        if status.is_ok() {
            crate::log!(INFO, "rename filters on");
        } else {
            crate::log!(WARN, "rename filters off: refused ({})", status.0);
        }
    }
}

/// `text` cut to at most `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Gives `entity` the label `text`; on the game thread only.
pub(crate) fn set(loader: &Loader, entity: u32, text: &str) -> Status {
    let Some(layout) = layout(loader) else {
        return Status::MISSING;
    };
    let server = match crate::world::server() {
        Ok(server) => server,
        Err(status) => return status,
    };
    // A dead entity has no template; never add a component to it.
    if crate::world::template(loader, entity).is_err() {
        return Status::MISSING;
    }
    // SAFETY: the server holds its world and its string table.
    let world: usize = unsafe { crate::world::read(server, layout.world) };
    // SAFETY: as above.
    let strings: usize = unsafe { crate::world::read(server, layout.strings) };
    if world == 0 || strings == 0 {
        return Status::MISSING;
    }
    let world = core::ptr::with_exposed_provenance_mut::<u8>(world);
    let text = cut(text, layout.max_text);
    let mut found = [0usize; 4];
    // SAFETY: the game's component lookup, on the game thread outside systems.
    let slot = unsafe { (layout.find)(&raw mut found, world, layout.label_type, entity) };
    // SAFETY: the lookup returns its out record, whose first word is the
    // component or null.
    let component = core::ptr::with_exposed_provenance_mut::<u64>(unsafe { slot.read() });
    let old = if component.is_null() {
        0
    } else {
        // SAFETY: a live CustomPlayerString is its string id.
        unsafe { component.read_unaligned() }
    };
    let span = [text.as_ptr().expose_provenance(), text.len()];
    let mut result = [0u64; 2];
    // SAFETY: the game's string swap through its chain, with the table the
    // rename path passes it; on the game thread.
    unsafe {
        (layout.replace)(
            &raw mut result,
            core::ptr::with_exposed_provenance_mut(strings),
            old,
            &raw const span,
        );
    };
    if result[0] & 0xff != 0 {
        return Status::REFUSED;
    }
    let id = result[1];
    if !component.is_null() {
        // SAFETY: as above; the rename path writes the id the same way.
        unsafe { component.write_unaligned(id) };
    } else if id != 0 {
        let data = [(&raw const id).expose_provenance(), size_of::<u64>()];
        // SAFETY: the game's add-component, as the rename path calls it.
        let _added = unsafe { (layout.add)(world, entity, layout.label_type, &raw const data) };
    }
    Status::OK
}

fn replace_next() -> ReplaceFn {
    let address = REPLACE_NEXT.load(Ordering::Acquire);
    // SAFETY: the loader points the slot at the rest of the chain, which has
    // the string swap's signature, before the link can run.
    unsafe { core::mem::transmute_copy::<usize, ReplaceFn>(&address) }
}

unsafe extern "system" fn replace(
    out: *mut [u64; 2],
    strings: *mut u8,
    old: u64,
    text: *const [usize; 2],
) -> *mut u8 {
    let refused = std::panic::catch_unwind(|| {
        // SAFETY: the game's arguments at the swap's entry.
        unsafe { refused(text) }
    });
    if matches!(refused, Ok(true)) && !out.is_null() {
        // SAFETY: the game's result record; its first byte is the error.
        unsafe { out.cast::<u8>().write(REFUSED) };
        return out.cast();
    }
    // SAFETY: forwarded unchanged.
    unsafe { replace_next()(out, strings, old, text) }
}

/// Whether a filter refuses this swap, when it is a player's rename.
///
/// # Safety
///
/// The game's text argument at the swap's entry.
unsafe fn refused(text: *const [usize; 2]) -> bool {
    let (Some(loader), Some(filters)) = (LOADED.get(), SEALED.get()) else {
        return false;
    };
    let Some(layout) = layout(loader) else {
        return false;
    };
    let frame = (loader.caller_frame)();
    // SAFETY: the frame was taken at this chain's entry, on this thread.
    if frame.rsp == 0 || unsafe { frame.return_address() } != layout.rename_return {
        return false;
    }
    let (Ok(rbp), Ok(rdi)) = (usize::try_from(frame.rbp), usize::try_from(frame.rdi)) else {
        return false;
    };
    if rbp == 0 || rdi == 0 || text.is_null() {
        return false;
    }
    // SAFETY: the rename message lies in the caller's frame; its first field
    // is the entity.
    let entity: u32 = unsafe { crate::world::read(rbp, layout.rename_entity) };
    // SAFETY: the caller walks its connections' handles with `rdi`.
    let machine: u32 = unsafe { crate::world::read(rdi, 0) };
    // SAFETY: the game's text span for the call.
    let [ptr, len] = unsafe { text.read() };
    let name = if ptr == 0 || len > 0x1000 {
        String::new()
    } else {
        // SAFETY: as above.
        let bytes = unsafe {
            core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(ptr), len)
        };
        String::from_utf8_lossy(bytes).into_owned()
    };
    let player = renamer(loader, machine);
    let call = RenameCall {
        header: Header::of::<RenameCall>(),
        entity,
        known: u8::from(player.is_some()),
        player: player.unwrap_or(PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: u32::MAX,
            entity: 0,
            key: 0,
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }),
        text: Str::new(&name),
    };
    let refused = filters
        .iter()
        .filter(|f| {
            loader
                .mods
                .get(f.owner)
                .is_some_and(|m| !m.off.load(Ordering::Acquire))
        })
        .any(|f| {
            let user = core::ptr::with_exposed_provenance_mut(f.user);
            // SAFETY: the filter lives in the mod's DLL, which stays loaded;
            // the safe layer catches its panics.
            let allowed = unsafe { (f.callback)(&raw const call, user) };
            allowed == 0
        });
    if refused {
        crate::log!(INFO, "rename of entity {entity} refused");
    }
    refused
}

/// The player on the connection `machine`.
fn renamer(loader: &Loader, machine: u32) -> Option<PlayerInfo> {
    let key = crate::world::machines(loader)
        .ok()?
        .into_iter()
        .find_map(|(key, m)| (m == machine).then_some(key))?;
    crate::world::players(loader)
        .ok()?
        .into_iter()
        .find(|p| p.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_cut_at_character_boundaries() {
        let long = "Сейф · ".repeat(40);
        let kept = cut(&long, 0xff);
        assert!(kept.len() <= 0xff);
        assert!(long.starts_with(kept));
    }
}

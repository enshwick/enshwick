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
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use enshwick::Entity;
use enshwick::abi::{Header, HookFlags, PlayerInfo, RenameCall, RenameCallFn, Status, Str};

use crate::chains::Next;
use crate::host::Loader;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::{Constant, Symbol};
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::label::ADD.id(),
    pinned::label::FIND.id(),
    pinned::label::MAX_TEXT.id(),
    pinned::label::RENAME_ENTITY.id(),
    pinned::label::RENAME_RETURN.id(),
    pinned::label::REPLACE.id(),
    pinned::server::CUSTOM_STRINGS.id(),
    pinned::server::WORLD.id(),
];

/// The game's error for a string it could not take; the rename path then
/// changes nothing.
const REFUSED: u8 = 0x1f;

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
    owner: ModIndex,
    callback: RenameCallFn,
    user: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
static LOADED: OnceLock<&'static Loader> = OnceLock::new();
// SAFETY: `ReplaceFn` is the exact signature of the game's string swap at the
// table's `label.replace`, and the slot is used only as the `next` of the
// `replace` link on that function.
static REPLACE_NEXT: Next<ReplaceFn> = unsafe { Next::new() };
static FILTERS: Registry<Filter> = Registry::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let symbol = |name: Symbol| {
                loader
                    .build
                    .symbol(name.id())
                    .and_then(|rva| usize::try_from(loader.base + rva).ok())
            };
            let constant = |name: Constant| {
                loader
                    .build
                    .constant(name.id())
                    .and_then(|v| usize::try_from(v).ok())
            };
            let (find, replace, add) = (
                symbol(pinned::label::FIND)?,
                symbol(pinned::label::REPLACE)?,
                symbol(pinned::label::ADD)?,
            );
            let descriptor = loader
                .reflection
                .type_info(crate::catalogue::type_name(
                    crate::catalogue::ids::CUSTOM_PLAYER_STRING,
                ))
                .ok()?
                .va();
            Some(Layout {
                // SAFETY: the build table's `label.find` is the address of
                // the game function with the `FindFn` signature on this build.
                find: unsafe { core::mem::transmute::<usize, FindFn>(find) },
                // SAFETY: the build table's `label.replace` is the address of
                // the game function with the `ReplaceFn` signature on this
                // build.
                replace: unsafe { core::mem::transmute::<usize, ReplaceFn>(replace) },
                // SAFETY: the build table's `label.add` is the address of the
                // game function with the `AddFn` signature on this build.
                add: unsafe { core::mem::transmute::<usize, AddFn>(add) },
                label_type: crate::world::type_hash(descriptor)?,
                world: constant(pinned::server::WORLD)?,
                strings: constant(pinned::server::CUSTOM_STRINGS)?,
                max_text: constant(pinned::label::MAX_TEXT)?,
                rename_return: loader.base
                    + loader.build.symbol(pinned::label::RENAME_RETURN.id())?,
                rename_entity: constant(pinned::label::RENAME_ENTITY)?,
            })
        })
        .as_ref()
}

/// Registers mod `owner`'s rename filter.
pub(crate) fn register(owner: ModIndex, callback: RenameCallFn, user: *mut c_void) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    FILTERS
        .add(Filter {
            owner,
            callback,
            user: user.expose_provenance(),
        })
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Links the rename check when a mod filters renames.
pub(crate) fn link(loader: &'static Loader) {
    let filters = FILTERS.seal();
    let wanted = !filters.is_empty();
    let _first = LOADED.set(loader);
    if !wanted {
        return;
    }
    let Some(layout) = layout(loader) else {
        crate::log!(WARN, "rename filters off: this build has no label layout");
        return;
    };
    let target = (layout.replace as *const ()).addr() as u64;
    let status = loader.link(target, &REPLACE_NEXT, replace, HookFlags::CALLER_FRAME);
    if status.is_ok() {
        crate::log!(INFO, "rename filters on");
    } else {
        crate::log!(WARN, "rename filters off: refused ({})", status.0);
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
pub(crate) fn set(loader: &Loader, entity: Entity, text: &str) -> Status {
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
    // SAFETY: `world::server` returned the live server on its game thread at
    // a quiet moment, when nothing else writes it; it holds its world pointer
    // at the table's `server.world`.
    let world = unsafe { crate::memory::read_ptr::<u8>(server, layout.world) };
    // SAFETY: `world::server` returned the live server on its game thread at
    // a quiet moment, when nothing else writes it; it holds its string table
    // pointer at the table's `server.custom_strings`.
    let strings = unsafe { crate::memory::read_ptr::<u8>(server, layout.strings) };
    if world.is_null() || strings.is_null() {
        return Status::MISSING;
    }
    let text = cut(text, layout.max_text);
    let mut found = [0usize; 4];
    // SAFETY: the game's component lookup at the table's `label.find`, with
    // the non-null world checked earlier and our own four-word out record, on
    // the game thread at a quiet moment, outside systems.
    let slot = unsafe { (layout.find)(&raw mut found, world, layout.label_type, entity.0) };
    // SAFETY: the lookup returns its out record, whose first word is the
    // component pointer or null.
    let component = unsafe { enshwick::raw::read_ptr_at::<u8>(slot.cast_const().cast(), 0) };
    let old = if component.is_null() {
        0
    } else {
        // SAFETY: `component` is the entity's live `CustomPlayerString`,
        // non-null by the earlier check, on the game thread at a quiet moment;
        // the component is its `u64` string id.
        unsafe { enshwick::raw::read_at::<u64>(component, 0) }
    };
    let span = [text.as_ptr().expose_provenance(), text.len()];
    let mut result = [0u64; 2];
    // SAFETY: the rest of the string swap chain at the table's
    // `label.replace`, with our own result record, the server's non-null
    // string table checked earlier and a span over `text`, which outlives the
    // call; on the game thread at a quiet moment.
    unsafe { (layout.replace)(&raw mut result, strings, old, &raw const span) };
    if result[0] & 0xff != 0 {
        return Status::REFUSED;
    }
    let id = result[1];
    if !component.is_null() {
        // SAFETY: `component` is the entity's live `CustomPlayerString`,
        // non-null by the check, on the game thread at a quiet moment; it
        // holds the `u64` string id, which the game's rename path writes the
        // same way.
        unsafe { enshwick::raw::write_at(component, 0, id) };
    } else if id != 0 {
        let data = [(&raw const id).expose_provenance(), size_of::<u64>()];
        // SAFETY: the game's add-component at the table's `label.add`, with
        // the non-null world checked earlier and a span over `id`, which
        // outlives the call; called as the game's rename path calls it, on the
        // game thread at a quiet moment.
        let _added = unsafe { (layout.add)(world, entity.0, layout.label_type, &raw const data) };
    }
    Status::OK
}

/// The switch of the label renames.
static LABELS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("label renames");

unsafe extern "system" fn replace(
    out: *mut [u64; 2],
    strings: *mut u8,
    old: u64,
    text: *const [usize; 2],
) -> *mut u8 {
    let refused = LABELS_FUSE.run("the replace", || {
        // SAFETY: `text` is the swap's own text argument, null or a span the
        // game keeps for the call, and this runs at the chain's entry on the
        // thread the game called the swap on.
        unsafe { refused(text) }
    });
    if matches!(refused, Some(true)) && !out.is_null() {
        // SAFETY: `out` is the swap's non-null result record (checked earlier),
        // writable for the call; its first byte is the error code.
        unsafe { enshwick::raw::write_at(out.cast(), 0, REFUSED) };
        return out.cast();
    }
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the string swap's signature.
    unsafe { REPLACE_NEXT.get()(out, strings, old, text) }
}

/// Whether a filter refuses this swap, when it is a player's rename.
///
/// # Safety
///
/// - `text` is null, or readable for a `[usize; 2]` span whose first word is
///   null or the address of `len` readable bytes, unchanged during the call.
/// - The call runs at the entry of the string swap chain, on the thread the
///   game called the swap on, so the loader's caller frame is the swap's.
unsafe fn refused(text: *const [usize; 2]) -> bool {
    let (Some(loader), Some(filters)) = (LOADED.get(), FILTERS.sealed()) else {
        return false;
    };
    let Some(layout) = layout(loader) else {
        return false;
    };
    let frame = (loader.caller_frame)();
    // SAFETY: by the caller's contract the frame was taken at this chain's
    // entry on this thread, so a non-zero `rsp` (checked first) points at the
    // swap's return address on the live stack.
    if frame.rsp == 0 || unsafe { frame.return_address() } != layout.rename_return {
        return false;
    }
    let (Ok(rbp), Ok(rdi)) = (usize::try_from(frame.rbp), usize::try_from(frame.rdi)) else {
        return false;
    };
    if rbp == 0 || rdi == 0 || text.is_null() {
        return false;
    }
    // SAFETY: the return address matched the table's `label.rename_return`,
    // so the caller is the game's rename path, whose non-zero `rbp` (checked
    // earlier) frames the rename message holding the `u32` entity at the
    // table's `label.rename_entity`; the frame is live while the swap runs.
    let entity: u32 = unsafe { crate::memory::read(rbp, layout.rename_entity) };
    // SAFETY: the caller is the game's rename path (return address checked
    // earlier), which holds the `u32` handle of the renaming connection at its
    // non-zero `rdi` (checked earlier) for the call.
    let machine: u32 = unsafe { crate::memory::read(rdi, 0) };
    // SAFETY: the caller's contract makes the non-null `text` (checked
    // earlier) a readable `[usize; 2]` span for the call.
    let [ptr, len] = unsafe { enshwick::raw::read_at::<[usize; 2]>(text.cast(), 0) };
    let name = if ptr == 0 || len > 0x1000 {
        String::new()
    } else {
        let mut bytes = vec![0; len];
        // SAFETY: the caller's contract makes the span's non-null first word
        // (checked earlier) the address of `len` readable bytes, unchanged
        // during the call.
        unsafe {
            enshwick::raw::read_bytes_at(core::ptr::with_exposed_provenance(ptr), 0, &mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let player = renamer(loader, machine);
    let call = RenameCall {
        header: Header::of::<RenameCall>(),
        entity: Entity(entity),
        known: u8::from(player.is_some()),
        player: player.unwrap_or(PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: u32::MAX,
            entity: Entity(0),
            key: enshwick::PlayerKey::new(0),
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
                .get(f.owner.get())
                .is_some_and(|m| !m.off.load(Ordering::Acquire))
        })
        .any(|f| {
            let user = core::ptr::with_exposed_provenance_mut(f.user);
            // SAFETY: the filter is the mod's registered `RenameCallFn` in
            // its DLL, which stays loaded for the process, called with a
            // `RenameCall` that outlives the call and the mod's own `user`
            // word; the mod's safe layer catches its panics.
            let allowed =
                crate::profile::call(f.owner, crate::profile::Feature::Renames, || unsafe {
                    (f.callback)(&raw const call, user)
                });
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
        .find(|p| p.key == enshwick::PlayerKey::new(key))
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

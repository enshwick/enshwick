//! Running mods' patches of a served resource: its first build, a rebuild
//! over the copy the game holds, and a build for a mod that reads a key as
//! served.
//!
//! A patch may build its output in a buffer the loader lends it
//! (`HostApi::resource_buffer`). The build that runs the patch owns the
//! buffer: the game's first build keeps the one its last output lies in for
//! the process, and frees the rest; a rebuild and a mod's served read free
//! all of them once they have written or copied the bytes.
//!
//! A build that reads another served key as served is built again whenever
//! that key is: the loader keeps its base for it.

use core::cell::RefCell;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use std::sync::atomic::Ordering;

use enshwick_core::abi::{ResourceKey, Status};

use super::{
    LOADED, OPEN_NEXT, PENDING, RESOLVE_NEXT, RESOURCES_FUSE, SERVED_LIST, Served, State, hex,
    key_name, mod_on, route,
};
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// For mod `owner`, which serves `key`: during init, keeps its base when it
/// is first built; afterwards, builds it again from that base and writes the
/// bytes that differ over the resource the game holds, then does the same
/// for every served key whose build read it.
pub(crate) fn rebuild(owner: ModIndex, key: ResourceKey) -> Status {
    let serves = |s: &Served| s.key == key && s.patches.iter().any(|p| p.owner == owner);
    let Some(list) = SERVED_LIST.get() else {
        let pending = PENDING.relock();
        return match pending.iter().find(|s| serves(s)) {
            Some(entry) => {
                entry.keeps_base.store(true, Ordering::Relaxed);
                Status::OK
            }
            None => Status::BAD_ARGS,
        };
    };
    match list.iter().find(|s| serves(s)) {
        Some(entry) => rebuild_entry(list, entry),
        None => Status::BAD_ARGS,
    }
}

/// Builds `entry` of `list` again, then the keys whose builds read it when
/// its served value may have changed: it changed, or the game never built
/// it, so only served reads saw it.
fn rebuild_entry(list: &[Served], entry: &Served) -> Status {
    match rebuild_one(entry) {
        Ok(Some(0)) => Status::OK,
        Ok(_) => {
            rebuild_readers(list, entry);
            Status::OK
        }
        Err(status) => status,
    }
}

/// Builds `entry` again from its kept base and writes the bytes that differ
/// over those the game holds; how many differed, `None` before its first
/// build, which takes the values in force.
fn rebuild_one(entry: &Served) -> Result<Option<usize>, Status> {
    let _one_at_a_time = entry.rebuilding.relock();
    // The game resolves the resource meanwhile: the patches run outside the
    // state's lock, on a copy of the base.
    let (data, len, base, built_by) = {
        let state = entry.state.relock();
        let Some((data, len)) = state.built else {
            return Ok(None);
        };
        let Some(base) = state.base.clone() else {
            return Err(Status::MISSING);
        };
        (data, len, base, state.built_by.clone())
    };
    let built = run_patches(
        entry,
        base.as_ptr().expose_provenance(),
        base.len(),
        Some(&built_by),
        Build::Game,
    )
    .ok_or(Status::MISSING)?;
    if built.len != len {
        crate::log!(
            WARN,
            "served resource {:08x}:{} not rebuilt: {} bytes, the game holds {len}",
            entry.key.type_hash,
            hex(&entry.key.guid),
            built.len
        );
        return Err(Status::REFUSED);
    }
    let changed = write_differences(data, built.data, len);
    crate::log!(
        INFO,
        "served resource {:08x}:{} rebuilt, {changed} bytes changed",
        entry.key.type_hash,
        hex(&entry.key.guid)
    );
    Ok(Some(changed))
}

/// Rebuilds the keys of `list` whose builds read `entry`, which changed, and
/// in turn those that read a key that changed; each once, so keys that read
/// each other end. A key whose build runs on this thread is left: its build
/// holds its state, and it reads `entry` as it is when it does.
fn rebuild_readers(list: &[Served], entry: &Served) {
    let mut done = vec![entry.key];
    let mut next = entry.read_by.relock().clone();
    while let Some(key) = next.pop() {
        if done.contains(&key) || building(&key) {
            continue;
        }
        done.push(key);
        let Some(reader) = list.iter().find(|s| s.key == key) else {
            continue;
        };
        if rebuild_one(reader).is_ok_and(|changed| changed != Some(0)) {
            next.extend(reader.read_by.relock().iter().copied());
        }
    }
}

/// The game's first build of `entry` from its base, recorded in `state`:
/// the bytes built, which stay for the process, and the mods that built
/// them; the base is kept when a mod asked for it or a patch read another
/// served key. `None`, recorded as failed, when no patch built.
///
/// # Safety
///
/// - `data` holds `len` readable bytes, unchanged for the call.
pub(super) unsafe fn build_first(
    entry: &Served,
    state: &mut State,
    data: usize,
    len: usize,
) -> Option<(usize, usize)> {
    let Some(mut built) = run_patches(entry, data, len, None, Build::Game) else {
        state.failed = true;
        return None;
    };
    if built.reads || entry.keeps_base.load(Ordering::Relaxed) {
        let mut base = vec![0; len];
        // SAFETY: the caller guarantees `len` readable bytes at `data`,
        // unchanged for the call.
        unsafe { crate::engine::memory::read_bytes(data, 0, &mut base) };
        state.base = Some(base.into_boxed_slice());
    }
    state.built_by = core::mem::take(&mut built.by);
    let result = built.keep();
    state.built = Some(result);
    Some(result)
}

/// Resource `key` as the game's first read would build it now: a served
/// key, renamed keys included, built from its base in the archive with the
/// patches of the mods on, kept nowhere; any other key, and every key while
/// served resources are not linked or off, as the game opens it from the
/// archive. A patch that fails is skipped, its mod left on; when every
/// patch fails, a changed vanilla key reads as the archive has it and a new
/// key as missing. Refused before the registrations seal;
/// [`Status::BUILDING`] for a key whose patches run on this thread, which a
/// patch reading it would build again without end.
pub(crate) fn served_now(key: ResourceKey) -> Result<Vec<u8>, Status> {
    if SERVED_LIST.get().is_none() {
        return Err(Status::REFUSED);
    }
    let archive = |key: &ResourceKey| {
        crate::engine::archive::game()
            .and_then(|archive| archive.resource(key))
            .ok_or(Status::MISSING)
    };
    if !linked() {
        return archive(&key);
    }
    let (found, new, aliased) = route(&key);
    let Some((_, entry)) = found else {
        return archive(&new.or(aliased).unwrap_or(key));
    };
    if building(&entry.key) {
        return Err(Status::BUILDING);
    }
    read_served(entry, archive(&entry.base)?, archive)
}

/// `entry` built for a mod's served read from `base`, its base's bytes:
/// each patch of a mod on, a failed one skipped. When none built, the key
/// as `archive` has it: `base` itself for a key patched in place, and
/// [`Status::MISSING`] for a new one. The game's builds running on this
/// thread are noted as reading `entry`.
fn read_served(
    entry: &Served,
    base: Vec<u8>,
    archive: impl FnOnce(&ResourceKey) -> Result<Vec<u8>, Status>,
) -> Result<Vec<u8>, Status> {
    note_read(entry);
    match run_patches(
        entry,
        base.as_ptr().expose_provenance(),
        base.len(),
        None,
        Build::Scratch,
    ) {
        Some(built) => Ok(built.into_bytes()),
        None if entry.key == entry.base => Ok(base),
        None => archive(&entry.key),
    }
}

/// Notes that the game's builds running on this thread read `entry`, so a
/// rebuild of it rebuilds them.
fn note_read(entry: &Served) {
    BUILDING.with_borrow_mut(|frames| {
        for frame in frames.iter_mut().filter(|f| f.build == Build::Game) {
            frame.reads = true;
            let mut read_by = entry.read_by.relock();
            if !read_by.contains(&frame.key) {
                read_by.push(frame.key);
            }
        }
    });
}

/// Whether the game reads served resources: their links made and not off.
fn linked() -> bool {
    !RESOURCES_FUSE.is_off() && OPEN_NEXT.wired().is_some() && RESOLVE_NEXT.wired().is_some()
}

/// Whether patches of `key` run on this thread.
fn building(key: &ResourceKey) -> bool {
    BUILDING.with_borrow(|frames| frames.iter().any(|f| f.key == *key))
}

/// Whether patches run on this thread for a mod's served read, not for
/// the game: the loader's own patches then log nothing, as the game's
/// build logs once.
pub(crate) fn scratch() -> bool {
    BUILDING.with_borrow(|frames| frames.iter().any(|f| f.build == Build::Scratch))
}

/// `len` zeroed bytes for the output of the patch running on this thread,
/// owned by its build; `None` outside a build or when they do not allocate.
pub(crate) fn lend(len: usize) -> Option<*mut u8> {
    if BUILDING.with_borrow(Vec::is_empty) {
        return None;
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).ok()?;
    bytes.resize(len, 0);
    Some(hand_over(bytes.into_boxed_slice()))
}

/// Hands the loader's own patch output `built` to the build running on this
/// thread and writes where it lies to `out` and `out_len`.
///
/// # Safety
///
/// - `out` and `out_len` are writable for one value each, as `run_patches`
///   passes them.
pub(crate) unsafe fn hand_out(built: Vec<u8>, out: *mut *const u8, out_len: *mut usize) -> Status {
    let len = built.len();
    let at = hand_over(built.into_boxed_slice());
    // SAFETY: the caller guarantees `out` writable for one pointer.
    unsafe { out.write(at.cast_const()) };
    // SAFETY: the caller guarantees `out_len` writable for one `usize`.
    unsafe { out_len.write(len) };
    Status::OK
}

/// Gives `bytes` to the build running on this thread, which frees them once
/// nothing reads them; outside a build they stay for the process. Where
/// they lie.
fn hand_over(bytes: Box<[u8]>) -> *mut u8 {
    let buffer = Buffer::new(bytes);
    let at = buffer.0.cast::<u8>().as_ptr();
    BUILDING.with_borrow_mut(|frames| match frames.last_mut() {
        Some(frame) => frame.buffers.push(buffer),
        None => buffer.keep(),
    });
    at
}

/// Writes the bytes of the `len` bytes at `new` that differ over those at
/// `held`, which the game reads meanwhile; how many differed.
pub(super) fn write_differences(held: usize, new: usize, len: usize) -> usize {
    // SAFETY: `new` is a patch's build of `len` bytes, valid while its
    // build lasts.
    let new =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(new), len) };
    let held = core::ptr::with_exposed_provenance_mut::<u8>(held);
    let mut changed = 0;
    for (at, &byte) in new.iter().enumerate() {
        let slot = held.wrapping_add(at);
        // SAFETY: `held` is the resource the game reads, `len` bytes the
        // first build kept writable for the process (`ResourcePatchFn`),
        // and `at` is below `len`. A volatile read, which `enshwick_core::raw`
        // has not: the bytes are the build's own, read while the game reads
        // them.
        // ast-grep-ignore: game-memory-through-raw
        let old = unsafe { slot.read_volatile() };
        if old != byte {
            // SAFETY: `held` is the first build's `len` bytes, writable for
            // the process (`ResourcePatchFn`), and `at` is below `len`; one
            // byte at a time, so a read meanwhile sees each byte old or new.
            unsafe { slot.write_volatile(byte) };
            changed += 1;
        }
    }
    changed
}

/// Who a build of a served resource is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Build {
    /// The game's first build or a rebuild, whose bytes the game holds: a
    /// failed patch turns its mod off.
    Game,
    /// A mod's served read, kept nowhere: a failed patch is skipped and its
    /// mod stays on, since the game never read that build.
    Scratch,
}

/// One build of a served key running on this thread.
struct Frame {
    key: ResourceKey,
    build: Build,
    /// Whether a patch read another served key as served.
    reads: bool,
    /// The buffers the build's patches took for their outputs.
    buffers: Vec<Buffer>,
}

thread_local! {
    /// The builds of served keys running on this thread, innermost last.
    static BUILDING: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
    /// How many builds of served keys this thread has begun.
    static BEGUN: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// How many builds of served keys this thread has begun, so bytes read
/// before one began are known to predate it.
pub(crate) fn builds_begun() -> u64 {
    BEGUN.get()
}

/// Begins a game build of `key` on this thread, which ends on drop.
#[cfg(test)]
pub(crate) fn enter_build(key: ResourceKey) -> impl Sized {
    Building::enter(key, Build::Game)
}

#[cfg(test)]
thread_local! {
    /// How many buffers are alive on this thread, kept ones included.
    static LIVE: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Bytes the loader owns for a patch's output, freed on drop unless kept.
struct Buffer(NonNull<[u8]>);

impl Buffer {
    fn new(bytes: Box<[u8]>) -> Self {
        #[cfg(test)]
        LIVE.set(LIVE.get() + 1);
        Self(NonNull::from(Box::leak(bytes)))
    }

    fn at(&self) -> usize {
        self.0.cast::<u8>().as_ptr().expose_provenance()
    }

    /// Whether the `len` bytes at `data` lie in the buffer.
    fn holds(&self, data: usize, len: usize) -> bool {
        data >= self.at()
            && data
                .checked_add(len)
                .is_some_and(|end| end <= self.at() + self.0.len())
    }

    /// Leaves the bytes for the process: the game holds them.
    fn keep(self) {
        let _kept = ManuallyDrop::new(self);
    }

    fn into_box(self) -> Box<[u8]> {
        #[cfg(test)]
        LIVE.set(LIVE.get() - 1);
        let this = ManuallyDrop::new(self);
        // SAFETY: `this.0` came from `Box::leak` in `new`, and `ManuallyDrop`
        // keeps `drop` from freeing it again.
        unsafe { Box::from_raw(this.0.as_ptr()) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        #[cfg(test)]
        LIVE.set(LIVE.get() - 1);
        // SAFETY: `self.0` came from `Box::leak` in `new`, and only this drop,
        // `keep` and `into_box` take it, each once.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

/// Marks a key's patches running on this thread until dropped, unwinding
/// included; the buffers its patches took go with it.
struct Building;

impl Building {
    fn enter(key: ResourceKey, build: Build) -> Self {
        BEGUN.set(BEGUN.get().wrapping_add(1));
        BUILDING.with_borrow_mut(|frames| {
            frames.push(Frame {
                key,
                build,
                reads: false,
                buffers: Vec::new(),
            });
        });
        Self
    }

    /// Ends the build: the buffers its patches took, and whether one read
    /// another served key.
    fn leave(self) -> (Vec<Buffer>, bool) {
        let left = BUILDING.with_borrow_mut(|frames| {
            frames.last_mut().map_or_else(
                || (Vec::new(), false),
                |frame| (core::mem::take(&mut frame.buffers), frame.reads),
            )
        });
        drop(self);
        left
    }
}

impl Drop for Building {
    fn drop(&mut self) {
        BUILDING.with_borrow_mut(Vec::pop);
    }
}

/// A served key's build: its bytes, who built them, and the loader's buffer
/// they lie in when a patch built them in one, freed with the build unless
/// kept.
pub(super) struct Built {
    pub(super) data: usize,
    pub(super) len: usize,
    pub(super) by: Vec<ModIndex>,
    /// Whether a patch read another served key as served.
    pub(super) reads: bool,
    owned: Option<Buffer>,
}

impl Built {
    /// Leaves the bytes for the process, as the game holds them; where they
    /// lie and their length.
    pub(super) fn keep(self) -> (usize, usize) {
        if let Some(buffer) = self.owned {
            buffer.keep();
        }
        (self.data, self.len)
    }

    /// The bytes, taken over without a copy when they fill a buffer of the
    /// loader's.
    fn into_bytes(self) -> Vec<u8> {
        match self.owned {
            Some(buffer) if buffer.at() == self.data && buffer.0.len() == self.len => {
                buffer.into_box().into_vec()
            }
            _ => {
                let mut bytes = vec![0; self.len];
                // SAFETY: `data` is the last patch's `len` built bytes: in the
                // loader's buffer this build still owns, or the patch's own,
                // valid for the process (`ResourcePatchFn`).
                unsafe { crate::engine::memory::read_bytes(self.data, 0, &mut bytes) };
                bytes
            }
        }
    }
}

/// Runs the patches of the mods still on, or of the mods `only` names,
/// over the `len` bytes at `data`, each on the result of the one before,
/// for `kind`; a failed one is skipped, and in the game's build turns its
/// mod off. `None` when no patch built.
pub(super) fn run_patches(
    entry: &Served,
    data: usize,
    len: usize,
    only: Option<&[ModIndex]>,
    kind: Build,
) -> Option<Built> {
    let building = Building::enter(entry.key, kind);
    let (mut current, mut current_len) = (data, len);
    let mut applied = Vec::new();
    let runs = |owner: ModIndex| only.map_or_else(|| mod_on(owner), |only| only.contains(&owner));
    for patch in entry.patches.iter().filter(|p| runs(p.owner)) {
        let (mut built, mut built_len) = (core::ptr::null::<u8>(), 0usize);
        // SAFETY: `patch.apply` is a live mod's or the loader's
        // `ResourcePatchFn`, given the `current_len` bytes so far at
        // `current`, valid for the call, the user pointer it registered and
        // out pointers to locals on this stack.
        let mut apply = || unsafe {
            (patch.apply)(
                core::ptr::with_exposed_provenance(current),
                current_len,
                core::ptr::with_exposed_provenance_mut(patch.user),
                &raw mut built,
                &raw mut built_len,
            )
        };
        let status = if patch.owner == ModIndex::LOADER {
            apply()
        } else {
            crate::engine::profile::call(
                patch.owner,
                crate::engine::profile::Feature::Resources,
                apply,
            )
        };
        if status != Status::OK || built.is_null() {
            if kind == Build::Scratch {
                continue;
            }
            if patch.owner == ModIndex::LOADER {
                crate::log!(
                    WARN,
                    "the loader's own patch of {} failed to build",
                    key_name(&entry.key)
                );
            } else if let Some(loader) = LOADED.get() {
                loader.turn_off(
                    patch.owner,
                    &format!(
                        "served {} failed to build ({})",
                        key_name(&entry.key),
                        status.0
                    ),
                );
            }
            continue;
        }
        (current, current_len) = (built.expose_provenance(), built_len);
        applied.push(patch.owner);
    }
    let (mut buffers, reads) = building.leave();
    if applied.is_empty() {
        return None;
    }
    let owned = buffers
        .iter()
        .position(|buffer| buffer.holds(current, current_len))
        .map(|at| buffers.swap_remove(at));
    Some(Built {
        data: current,
        len: current_len,
        by: applied,
        reads,
        owned,
    })
}

#[cfg(test)]
mod tests;

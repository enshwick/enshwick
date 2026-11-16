//! Running mods' patches of a served resource: its first build, a rebuild
//! over the copy the game holds, and a build for a mod that reads a key as
//! served.
//!
//! A patch builds its output in a buffer the loader lends it
//! (`HostApi::resource_buffer`), the only place an output may lie. The
//! build that runs the patch owns the buffer: the game's first build keeps
//! the one its last output lies in for the process, and frees the rest; a
//! rebuild and a mod's served read free all of them once they have written
//! or copied the bytes.
//!
//! A build that reads another served key as served is built again whenever
//! that key is: the loader keeps its base for it.

use core::cell::RefCell;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use std::sync::atomic::Ordering;

use enshwick_core::abi::{ResourceKey, Status};

use super::ServedError;

use super::{
    OPEN_NEXT, Patch, RESOLVE_NEXT, RESOURCES, RESOURCES_FUSE, SERVING_FUSE, Served, State, hex,
    key_name, mod_on, route,
};
use crate::engine::fuse::Fuse;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// For mod `owner`, which serves `key`: during init, keeps its base when it
/// is first built; afterwards, builds it again from that base and writes the
/// bytes that differ over the resource the game holds, then does the same
/// for every served key whose build read it.
pub(crate) fn rebuild(owner: ModIndex, key: ResourceKey) -> Result<(), ServedError> {
    let serves = |s: &Served| s.key == key && s.patches.iter().any(|p| p.owner == owner);
    let Some(list) = RESOURCES.sealed() else {
        let pending = RESOURCES.edit(|pending| match pending.iter().find(|s| serves(s)) {
            Some(entry) => {
                entry.keeps_base.store(true, Ordering::Relaxed);
                Ok(())
            }
            None => Err(ServedError::NotServed),
        });
        // Sealed since the look before: rebuild from the sealed list.
        return pending.unwrap_or_else(|_| rebuild(owner, key));
    };
    match list.iter().find(|s| serves(s)) {
        Some(entry) => rebuild_entry(list, entry),
        None => Err(ServedError::NotServed),
    }
}

/// Builds `entry` of `list` again, then the keys whose builds read it when
/// its served value may have changed: it changed, or the game never built
/// it, so only served reads saw it. [`ServedError::Building`] while `entry`'s
/// patches run on this thread: a patch asking to rebuild its own key would
/// wait on its own build.
fn rebuild_entry(list: &[Served], entry: &Served) -> Result<(), ServedError> {
    if building(&entry.key) {
        return Err(ServedError::Building);
    }
    if rebuild_one(entry)? != Some(0) {
        rebuild_readers(list, entry);
    }
    Ok(())
}

/// Builds `entry` again from its kept base and writes the bytes that differ
/// over those the game holds; how many differed, `None` before its first
/// build, which takes the values in force.
fn rebuild_one(entry: &Served) -> Result<Option<usize>, ServedError> {
    let _one_at_a_time = entry.rebuilding.relock();
    // The game resolves the resource meanwhile: the patches run outside the
    // state's lock, on a copy of the base.
    let (data, len, base, built_by) = {
        let state = entry.state.relock();
        let Some((data, len)) = state.built else {
            return Ok(None);
        };
        let Some(base) = state.base.clone() else {
            return Err(ServedError::NoBase);
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
    .ok_or(ServedError::NotBuilt)?;
    if built.len != len {
        crate::log!(
            WARN,
            "served resource {:08x}:{} not rebuilt: {} bytes, the game holds {len}",
            entry.key.type_hash,
            hex(&entry.key.guid.0),
            built.len
        );
        return Err(ServedError::SizeChanged);
    }
    let changed = write_differences(data, built.data, len);
    crate::log!(
        INFO,
        "served resource {:08x}:{} rebuilt, {changed} bytes changed",
        entry.key.type_hash,
        hex(&entry.key.guid.0)
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
/// served key. Recorded as failed when no patch built: then a copy of the
/// base for a changed vanilla key, and `None` for a new one.
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
        if entry.key != entry.base {
            return None;
        }
        // A changed vanilla key whose every patch failed reads as the
        // archive has it: missing, it would take the game's own data away.
        let mut base = vec![0; len];
        // SAFETY: the caller guarantees `len` readable bytes at `data`,
        // unchanged for the call.
        unsafe { crate::engine::memory::read_bytes(data, 0, &mut base) };
        let kept = Buffer::new(base.into_boxed_slice());
        let result = (kept.at(), len);
        kept.keep();
        state.built = Some(result);
        return Some(result);
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
/// [`ServedError::Building`] for a key whose patches run on this thread, which a
/// patch reading it would build again without end.
pub(crate) fn served_now(key: ResourceKey) -> Result<Vec<u8>, ServedError> {
    if RESOURCES.sealed().is_none() {
        return Err(ServedError::Unsealed);
    }
    let archive = |key: &ResourceKey| {
        crate::engine::archive::game()
            .and_then(|archive| archive.resource(key))
            .ok_or(ServedError::NotInArchive)
    };
    if !linked() {
        return archive(&key);
    }
    let (found, new, aliased) = route(&key);
    let Some((_, entry)) = found else {
        return archive(&new.or(aliased).unwrap_or(key));
    };
    if building(&entry.key) {
        return Err(ServedError::Building);
    }
    read_served(entry, archive(&entry.base)?, archive)
}

/// `entry` built for a mod's served read from `base`, its base's bytes:
/// each patch of a mod on, a failed one skipped. When none built, the key
/// as `archive` has it: `base` itself for a key patched in place, and
/// [`ServedError::NotInArchive`] for a new one. The game's builds running on this
/// thread are noted as reading `entry`.
fn read_served(
    entry: &Served,
    base: Vec<u8>,
    archive: impl FnOnce(&ResourceKey) -> Result<Vec<u8>, ServedError>,
) -> Result<Vec<u8>, ServedError> {
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

/// Whether the game reads served resources: their links made, and neither
/// the links nor the work in them off.
fn linked() -> bool {
    !RESOURCES_FUSE.is_off()
        && !SERVING_FUSE.is_off()
        && OPEN_NEXT.wired().is_some()
        && RESOLVE_NEXT.wired().is_some()
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
    hand_over(bytes.into_boxed_slice())
}

/// Hands the loader's own patch output `built` to the build running on this
/// thread and writes where it lies to `out` and `out_len`;
/// [`Status::REFUSED`] outside a build or for an output over `u32::MAX`
/// bytes.
///
/// # Safety
///
/// - `out` and `out_len` are writable for one value each, as `run_patches`
///   passes them.
pub(crate) unsafe fn hand_out(built: Vec<u8>, out: *mut *mut u8, out_len: *mut u32) -> Status {
    let Ok(len) = u32::try_from(built.len()) else {
        return Status::REFUSED;
    };
    let Some(at) = hand_over(built.into_boxed_slice()) else {
        return Status::REFUSED;
    };
    // SAFETY: the caller guarantees `out` writable for one pointer.
    unsafe { out.write(at) };
    // SAFETY: the caller guarantees `out_len` writable for one `u32`.
    unsafe { out_len.write(len) };
    Status::OK
}

/// What every resource patch of the loader's own does around its edit:
/// `build` gets a copy of the `base_len` bytes at `base` and runs under
/// `fuse` as `site`, and the bytes it makes are handed out.
/// [`Status::REFUSED`] when it makes none; [`Status::FAULTED`] when it
/// panics, which turns its feature off, or its feature is off.
///
/// # Safety
///
/// - `base` is readable for `base_len` bytes, unchanged for the call, and
///   `out` and `out_len` are writable for one value each, as
///   [`ResourcePatchFn`](enshwick_core::abi::ResourcePatchFn) says.
pub(crate) unsafe fn patched(
    fuse: &Fuse,
    site: &str,
    (base, base_len): (*const u8, u32),
    (out, out_len): (*mut *mut u8, *mut u32),
    build: impl FnOnce(Vec<u8>) -> Option<Vec<u8>>,
) -> Status {
    let mut bytes = vec![0; base_len as usize];
    // SAFETY: the caller guarantees `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick_core::raw::read_bytes_at(base, 0, &mut bytes) };
    let Some(made) = fuse.run(site, || build(bytes)) else {
        return Status::FAULTED;
    };
    let Some(made) = made else {
        return Status::REFUSED;
    };
    // SAFETY: the caller guarantees `out` and `out_len` writable for one
    // value each.
    unsafe { hand_out(made, out, out_len) }
}

/// One of the loader's own resource patches: what it needs, leaked for the
/// process by [`own_patch`], and the edit it makes of a base.
pub(crate) trait OwnPatch: Sync + 'static {
    /// The site a panic of [`OwnPatch::build`] names.
    const SITE: &'static str;

    /// The switch the patch builds under. A patch of one key of many has a
    /// switch of its own, so a panic refuses that key alone and the other
    /// keys keep their changes, which the save would otherwise lose.
    fn fuse(&self) -> &Fuse;

    /// The resource `base` with the patch's edit; `None` leaves it be.
    fn build(&self, base: Vec<u8>) -> Option<Vec<u8>>;
}

/// The function and user value of a loader patch that builds with `patch`,
/// leaked for the process.
pub(crate) fn own_patch<P: OwnPatch>(
    patch: P,
) -> (enshwick_core::abi::ResourcePatchFn, *mut core::ffi::c_void) {
    (apply_own::<P>, Box::into_raw(Box::new(patch)).cast())
}

/// The patch [`own_patch`] made: [`OwnPatch::build`] under its switch, by
/// [`patched`].
unsafe extern "system" fn apply_own<P: OwnPatch>(
    base: *const u8,
    base_len: u32,
    user: *mut core::ffi::c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    // SAFETY: `user` is the `P` `own_patch` leaked with this function, which
    // lives for the process.
    let patch = unsafe { &*user.cast::<P>() };
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call, and `run_patches` passes `out` and `out_len`
    // pointing at locals, writable for the call.
    unsafe {
        patched(
            patch.fuse(),
            P::SITE,
            (base, base_len),
            (out, out_len),
            |bytes| patch.build(bytes),
        )
    }
}

/// Gives `bytes` to the build running on this thread, which frees them once
/// nothing reads them; where they lie, `None` outside a build.
fn hand_over(bytes: Box<[u8]>) -> Option<*mut u8> {
    let buffer = Buffer::new(bytes);
    let at = buffer.0.cast::<u8>().as_ptr();
    BUILDING.with_borrow_mut(|frames| frames.last_mut().map(|frame| frame.buffers.push(buffer)))?;
    Some(at)
}

/// Whether the `len` bytes at `data` lie in a buffer the build running on
/// this thread lent.
fn lent_holds(data: usize, len: usize) -> bool {
    BUILDING.with_borrow(|frames| {
        frames
            .last()
            .is_some_and(|frame| frame.buffers.iter().any(|buffer| buffer.holds(data, len)))
    })
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
        // SAFETY: `held` is the resource the game reads, `len` bytes of the
        // loader's buffer the first build kept writable for the process
        // (`Built::keep`), and `at` is below `len`. A volatile read, which `enshwick_core::raw`
        // has not: the bytes are the build's own, read while the game reads
        // them.
        // ast-grep-ignore: game-memory-through-raw
        let old = unsafe { slot.read_volatile() };
        if old != byte {
            // SAFETY: `held` is the first build's `len` bytes, writable for
            // the process (`Built::keep`), and `at` is below `len`; one
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

    fn bytes(&self) -> &[u8] {
        // SAFETY: `self.0` came from `Box::leak` in `new` and lives until
        // `self` goes; the patches that wrote it have returned.
        unsafe { self.0.as_ref() }
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
/// they lie in, freed with the build unless kept.
pub(super) struct Built {
    pub(super) data: usize,
    pub(super) len: usize,
    pub(super) by: Vec<ModIndex>,
    /// Whether a patch read another served key as served.
    pub(super) reads: bool,
    owned: Buffer,
}

impl Built {
    /// Leaves the bytes for the process, as the game holds them; where they
    /// lie and their length.
    pub(super) fn keep(self) -> (usize, usize) {
        self.owned.keep();
        (self.data, self.len)
    }

    /// The bytes, taken over without a copy when they fill their buffer.
    fn into_bytes(self) -> Vec<u8> {
        let start = self.data - self.owned.at();
        if start == 0 && self.owned.0.len() == self.len {
            return self.owned.into_box().into_vec();
        }
        self.owned.bytes()[start..start + self.len].to_vec()
    }
}

/// Runs the patches still on of the mods still on, or for a rebuild every
/// patch of the mods `only` names, over the `len` bytes at `data`, each on
/// the result of the one before, for `kind`; a failed one is skipped, and
/// in the game's build turns its mod off. `None` when no patch built.
pub(super) fn run_patches(
    entry: &Served,
    data: usize,
    len: usize,
    only: Option<&[ModIndex]>,
    kind: Build,
) -> Option<Built> {
    let Ok(len) = u32::try_from(len) else {
        crate::log!(
            WARN,
            "served {} not built: its base is over 4 GiB",
            key_name(&entry.key)
        );
        return None;
    };
    let building = Building::enter(entry.key, kind);
    let (mut current, mut current_len) = (data, len);
    let mut applied = Vec::new();
    // A rebuild runs every patch the game's build applied, one whose
    // builder panicked since included: the game holds what each added.
    let runs = |p: &Patch| match only {
        Some(only) => only.contains(&p.owner),
        None => p.runs(mod_on),
    };
    for patch in entry.patches.iter().filter(|p| runs(p)) {
        let (mut built, mut built_len) = (core::ptr::null_mut::<u8>(), 0u32);
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
        // An output outside the buffers this build lent, null included, is
        // a failure: the loader could neither free nor keep it.
        let lent = lent_holds(built.expose_provenance(), built_len as usize);
        let status = if status == Status::OK && !lent {
            Status::BAD_ARGS
        } else {
            status
        };
        if status == Status::FAULTED {
            fault(entry, patch);
            continue;
        }
        if status != Status::OK {
            if kind == Build::Scratch {
                continue;
            }
            if patch.owner == ModIndex::LOADER {
                crate::log!(
                    WARN,
                    "the loader's own patch of {} failed to build",
                    key_name(&entry.key)
                );
            } else if let Some(loader) = crate::engine::loader::loader() {
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
    let current_len = current_len as usize;
    // The last applied output passed `lent_holds`, so a buffer holds it.
    let at = buffers
        .iter()
        .position(|buffer| buffer.holds(current, current_len))?;
    Some(Built {
        data: current,
        len: current_len,
        by: applied,
        reads,
        owned: buffers.swap_remove(at),
    })
}

/// Turns off `patch` of `entry`, whose builder panicked, for the process,
/// and names the resource off for its mod: the loader's own patch is listed
/// by its fuse, which the panic turned off.
#[cold]
fn fault(entry: &Served, patch: &Patch) {
    if !patch.on.swap(false, Ordering::AcqRel) || patch.owner == ModIndex::LOADER {
        return;
    }
    if let Some(record) =
        crate::engine::loader::loader().and_then(|l| l.mods.get(patch.owner.get()))
    {
        record.report_feature(
            &format!("served {}", key_name(&entry.key)),
            Some(enshwick_core::resources::BUILDER_PANICKED),
        );
    }
}

#[cfg(test)]
mod tests;

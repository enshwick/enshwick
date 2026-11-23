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
//! that key is, and one that asks whether the client shows served UI images
//! before they show is built again once they show: the loader keeps its
//! base for it.

use core::cell::RefCell;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use std::sync::atomic::{Ordering, fence};

pub(super) use enshwick_base::__internal::Build;
use enshwick_base::__internal::{self as builds, BuildResourceError, PatchFailure, Unbuilt, View};
use enshwick_core::abi::{ResourceKey, Status};

use super::{
    ICONS_FUSE, OPEN_NEXT, Patch, RESOLVE_NEXT, RESOURCES, RESOURCES_FUSE, SERVING_FUSE, Served,
    State, hex, key_name, mod_on, route,
};
use crate::engine::fuse::Fuse;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// For mod `owner`, which serves `key`: during init, keeps its base when it
/// is first built; afterwards, builds it again from that base and writes the
/// bytes that differ over the resource the game holds, then does the same
/// for every served key whose build read it.
pub(crate) fn rebuild(owner: ModIndex, key: ResourceKey) -> Result<(), BuildResourceError> {
    if let Some(list) = RESOURCES.sealed() {
        return builds::rebuild(&Table::game(list), owner, &key);
    }
    let pending = RESOURCES.edit(|pending| {
        let table = Table {
            sealed: false,
            ..Table::game(pending)
        };
        builds::rebuild(&table, owner, &key)
    });
    // Sealed since the look before: rebuild from the sealed list.
    pending.unwrap_or_else(|_| rebuild(owner, key))
}

/// Builds `entry` of `list` again, then the keys whose builds read it, as
/// [`builds::rebuild_in`] does.
#[cfg(test)]
fn rebuild_entry(list: &[Served], entry: &Served) -> Result<(), BuildResourceError> {
    builds::rebuild_in(&Table::game(list), entry, &mut Vec::new())
}

/// Builds `entry` again over the bytes the game holds, as
/// [`builds::rebuild_one`] does.
#[cfg(test)]
fn rebuild_one(entry: &Served) -> Result<Option<usize>, BuildResourceError> {
    builds::rebuild_one(&Table::game(core::slice::from_ref(entry)), entry)
}

/// Rebuilds the keys of `list` that read `entry`, as
/// [`builds::rebuild_readers`] does.
#[cfg(test)]
fn rebuild_readers(list: &[Served], entry: &Served, done: &mut Vec<ResourceKey>) {
    builds::rebuild_readers(&Table::game(list), entry, done);
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
    // SAFETY: the caller guarantees `len` readable bytes at `data`,
    // unchanged for the call.
    let first = unsafe {
        builds::build_first(
            &Table::game(core::slice::from_ref(entry)),
            entry,
            (data, len),
        )
    };
    let copy_base = || {
        let mut base = vec![0; len];
        // SAFETY: the caller guarantees `len` readable bytes at `data`,
        // unchanged for the call.
        unsafe { crate::engine::memory::read_bytes(data, 0, &mut base) };
        base.into_boxed_slice()
    };
    let Ok(built) = first.built else {
        state.failed = true;
        if !first.holds {
            return None;
        }
        // A changed vanilla key whose every patch failed reads as the
        // archive has it: missing, it would take the game's own data away.
        let kept = Buffer::new(copy_base());
        let result = (kept.at(), len);
        kept.keep();
        state.built = Some(result);
        return Some(result);
    };
    if first.keep_base {
        state.base = Some(copy_base());
    }
    state.built_by = built.by;
    let result = (built.bytes.data, built.len);
    built.bytes.owned.keep();
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
/// [`BuildResourceError::Building`] for a key whose patches run on this thread, which a
/// patch reading it would build again without end.
pub(crate) fn served_now(key: ResourceKey) -> Result<Vec<u8>, BuildResourceError> {
    let table = RESOURCES.sealed().map_or(
        Table {
            sealed: false,
            ..Table::game(&[])
        },
        Table::game,
    );
    builds::read_served(&table, &key)
}

/// `entry` built for a mod's served read from `base`, its base's bytes, as
/// [`builds::read_built`] does, with `archive` for the archive's bytes.
#[cfg(test)]
fn read_served(
    entry: &Served,
    base: Vec<u8>,
    archive: impl Fn(&ResourceKey) -> Result<Vec<u8>, BuildResourceError>,
) -> Result<Vec<u8>, BuildResourceError> {
    let table = Table {
        list: core::slice::from_ref(entry),
        sealed: true,
        archive: move |key: &ResourceKey| archive(key).ok(),
        on: mod_on,
    };
    builds::read_built(&table, entry, base, Build::Scratch)
}

/// The game archive's resource `key`.
fn game_archive(key: &ResourceKey) -> Option<Vec<u8>> {
    crate::engine::archive::game().and_then(|archive| archive.resource(key))
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

/// Notes that the game's builds running on this thread ask whether this
/// client shows served UI images, which it does not yet, so [`icons_shown`]
/// builds them again. Called before the answer is read, so a build that
/// read "not yet" is noted before the announcement looks.
pub(crate) fn note_icons_read() {
    if let Some(list) = RESOURCES.sealed() {
        note_icons_in(list);
        // With the fence of `icons_shown`: either this thread then reads the
        // images as shown, or the announcement sees this note.
        fence(Ordering::SeqCst);
    }
}

/// [`note_icons_read`] for the served keys of `list`.
fn note_icons_in(list: &[Served]) {
    BUILDING.with_borrow(|frames| {
        for frame in frames.iter().filter(|f| f.build == Build::Game) {
            if let Some(entry) = list.iter().find(|s| s.key == frame.key) {
                entry.reads_icons.store(true, Ordering::SeqCst);
            }
        }
    });
}

/// Builds again every served key whose game build asked whether this
/// client shows served UI images before they showed, and the keys that
/// read those, now that they show; a rebuild writes only the bytes that
/// differ, so the length stays. Runs on the UI thread, which draws every
/// icon, so no icon GUID is read while its bytes change one at a time. A
/// panic turns off only this rebuild.
pub(crate) fn icons_shown() {
    if let Some(list) = RESOURCES.sealed() {
        // Pairs with the fence of `note_icons_read`.
        fence(Ordering::SeqCst);
        ICONS_FUSE.run("the rebuild once icons show", || {
            rebuild_icon_readers(list);
        });
    }
}

/// [`icons_shown`] for the served keys of `list`: each key once, also when
/// it reads another key built again, and then without the base it kept
/// only for this rebuild.
fn rebuild_icon_readers(list: &[Served]) {
    let mut done = Vec::new();
    for entry in list.iter().filter(|s| s.reads_icons.load(Ordering::SeqCst)) {
        if !done.contains(&entry.key)
            && let Err(why) = builds::rebuild_in(&Table::game(list), entry, &mut done)
        {
            crate::log!(
                WARN,
                "served {} does not show its icons: {why}",
                key_name(&entry.key)
            );
        }
        let reads = list.iter().any(|s| s.read_by.relock().contains(&entry.key));
        if !reads && !entry.keeps_base.load(Ordering::Relaxed) {
            entry.state.relock().base = None;
        }
    }
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
    BUILDING.with_borrow(|frames| frames.iter().any(|f| f.build != Build::Game))
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

/// The bytes of a finished build and the loader's buffer they lie in, freed
/// with them unless kept.
pub(super) struct Lent {
    data: usize,
    owned: Buffer,
}

impl Lent {
    /// The `len` bytes, taken over without a copy when they fill their
    /// buffer.
    fn into_bytes(self, len: usize) -> Vec<u8> {
        let start = self.data - self.owned.at();
        if start == 0 && self.owned.0.len() == len {
            return self.owned.into_box().into_vec();
        }
        self.owned.bytes()[start..start + len].to_vec()
    }
}

/// A served key's build as the tests read it: its bytes, who built them,
/// and the loader's buffer they lie in.
#[cfg(test)]
pub(super) struct Built {
    pub(super) len: usize,
    pub(super) by: Vec<ModIndex>,
    bytes: Lent,
}

#[cfg(test)]
impl Built {
    /// The bytes, taken over without a copy when they fill their buffer.
    fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_bytes(self.len)
    }
}

/// Runs the patches of `entry` over the `len` bytes at `data` for `kind`, as
/// [`builds::build`] picks them. `None` when no patch built.
#[cfg(test)]
pub(super) fn run_patches(
    entry: &Served,
    data: usize,
    len: usize,
    only: Option<&[ModIndex]>,
    kind: Build,
) -> Option<Built> {
    let table = Table::game(core::slice::from_ref(entry));
    // SAFETY: the tests pass `len` bytes at `data` that they hold for the
    // call, or over 4 GiB, which `build` refuses before reading.
    let built = unsafe { builds::build(&table, entry, (data, len), kind, only) }.ok()?;
    Some(Built {
        len: built.len,
        by: built.by,
        bytes: built.bytes,
    })
}

/// The served keys of `list` as the shared build routine reads them, with
/// `archive` for the archive's bytes; `sealed` once the registrations are.
struct Table<'a, A> {
    list: &'a [Served],
    sealed: bool,
    archive: A,
    /// Whether a patch's owner is on: [`mod_on`], but in a benchmark.
    on: fn(ModIndex) -> bool,
}

impl<'a> Table<'a, fn(&ResourceKey) -> Option<Vec<u8>>> {
    /// The sealed `list`, read from the game's archive.
    fn game(list: &'a [Served]) -> Self {
        Self {
            list,
            sealed: true,
            archive: game_archive,
            on: mod_on,
        }
    }
}

/// What a rebuild of a key holds while it builds: the key's rebuild lock, so
/// two never write over each other, the base its first build kept and where
/// the bytes the game holds lie.
struct Rebuilding<'a> {
    _one_at_a_time: std::sync::MutexGuard<'a, ()>,
    base: Box<[u8]>,
    held: usize,
}

// SAFETY: `apply` answers an output only when `lent_holds` finds it in a
// buffer of the running frame, which `Building` frees only once `leave` takes
// it.
unsafe impl<'a, A> builds::Patches for Table<'a, A> {
    type Key = &'a Served;
    type Owner = ModIndex;
    type Patch = &'a Patch;
    type Frame = Building;
    type Bytes = Lent;

    fn building(&self, key: &'a Served) -> bool {
        building(&key.key)
    }

    fn patches(&self, key: &'a Served) -> impl Iterator<Item = &'a Patch> {
        key.patches.iter()
    }

    fn owner(&self, patch: &&'a Patch) -> ModIndex {
        patch.owner
    }

    fn on(&self, patch: &&'a Patch) -> bool {
        patch.runs(self.on)
    }

    fn rekey(&self, patch: &&'a Patch) -> Option<ResourceKey> {
        patch.rekey
    }

    fn enter(&self, key: &'a Served, build: Build) -> Building {
        Building::enter(key.key, build)
    }

    unsafe fn apply(
        &self,
        patch: &&'a Patch,
        input: View,
        _rebuild: bool,
    ) -> (Status, Option<View>) {
        let (mut built, mut built_len) = (core::ptr::null_mut::<u8>(), 0u32);
        // SAFETY: `patch.apply` is a live mod's or the loader's
        // `ResourcePatchFn`, given the `input.len` bytes at `input.at`,
        // readable for the call as the caller guarantees, the user pointer it
        // registered and out pointers to locals on this stack.
        let mut apply = || unsafe {
            (patch.apply)(
                core::ptr::with_exposed_provenance(input.at),
                input.len,
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
        // An output outside the buffers this build lent, null included,
        // could be neither freed nor kept.
        let at = built.expose_provenance();
        let lent = lent_holds(at, built_len as usize);
        (status, lent.then_some(View { at, len: built_len }))
    }

    fn failed(&self, key: &'a Served, patch: &&'a Patch, why: PatchFailure) {
        match why {
            PatchFailure::Panicked => fault(key, patch),
            PatchFailure::Refused(status) => refused(key, patch, status),
        }
    }

    fn leave(&self, frame: Building, last: Option<View>) -> (bool, Option<Lent>) {
        let (mut buffers, reads) = frame.leave();
        let owned = last.and_then(|last| {
            let at = buffers
                .iter()
                .position(|buffer| buffer.holds(last.at, last.len as usize))?;
            Some(Lent {
                data: last.at,
                owned: buffers.swap_remove(at),
            })
        });
        (reads, owned)
    }

    fn unbuilt(&self, key: &'a Served, why: Unbuilt) {
        match why {
            Unbuilt::TooLarge => crate::log!(
                WARN,
                "served {} not built: its base is over 4 GiB",
                key_name(&key.key)
            ),
            Unbuilt::SizeChanged { built, held } => crate::log!(
                WARN,
                "served resource {:08x}:{} not rebuilt: {built} bytes, the game holds {held}",
                key.key.type_hash,
                hex(&key.key.guid.0)
            ),
        }
    }
}

// SAFETY: `base` answers the bytes of the kept `Rebuilding`'s boxed copy,
// which nothing changes while it lives.
unsafe impl<'a, A: Fn(&ResourceKey) -> Option<Vec<u8>>> builds::ServedTable for Table<'a, A> {
    type Kept = Rebuilding<'a>;

    fn sealed(&self) -> bool {
        self.sealed
    }

    fn find(&self, key: &ResourceKey) -> Result<&'a Served, ResourceKey> {
        if !linked() {
            return Err(*key);
        }
        let (found, new, aliased) = route(key);
        match found {
            Some((_, entry)) => Ok(entry),
            None => Err(new.or(aliased).unwrap_or(*key)),
        }
    }

    fn served_by(&self, owner: ModIndex, key: &ResourceKey) -> Option<&'a Served> {
        self.list
            .iter()
            .find(|s| s.key == *key && s.patches.iter().any(|p| p.owner == owner))
    }

    fn keys(&self, key: &'a Served) -> (ResourceKey, ResourceKey) {
        (key.key, key.base)
    }

    fn archived(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        (self.archive)(key)
    }

    fn keep_base(&self, key: &'a Served) {
        key.keeps_base.store(true, Ordering::Relaxed);
    }

    fn keeps_base(&self, key: &'a Served) -> builds::KeepBase {
        builds::KeepBase {
            asked: key.keeps_base.load(Ordering::Relaxed),
            asked_icons: key.reads_icons.load(Ordering::SeqCst),
            ..builds::KeepBase::default()
        }
    }

    fn note_read(&self, key: &'a Served) {
        note_read(key);
    }

    fn held(&self, key: &'a Served) -> Option<builds::Held<Rebuilding<'a>, ModIndex>> {
        let one_at_a_time = key.rebuilding.relock();
        // The game resolves the resource meanwhile: the patches run outside
        // the state's lock, on a copy of the base.
        let state = key.state.relock();
        let (held, len) = state.built?;
        let base = state.base.clone().map(|base| Rebuilding {
            _one_at_a_time: one_at_a_time,
            base,
            held,
        });
        Some(builds::Held {
            base,
            len,
            by: state.built_by.clone(),
        })
    }

    fn base(&self, kept: &Rebuilding<'a>) -> (usize, usize) {
        (kept.base.as_ptr().expose_provenance(), kept.base.len())
    }

    fn write(
        &self,
        key: &'a Served,
        kept: Rebuilding<'a>,
        built: builds::Built<Lent, ModIndex>,
    ) -> usize {
        let changed = write_differences(kept.held, built.bytes.data, built.len);
        crate::log!(
            INFO,
            "served resource {:08x}:{} rebuilt, {changed} bytes changed",
            key.key.type_hash,
            hex(&key.key.guid.0)
        );
        changed
    }

    fn readers(&self, key: &'a Served) -> Vec<&'a Served> {
        key.read_by
            .relock()
            .iter()
            .filter_map(|read| self.list.iter().find(|s| s.key == *read))
            .collect()
    }

    fn to_vec(&self, built: builds::Built<Lent, ModIndex>) -> Vec<u8> {
        built.bytes.into_bytes(built.len)
    }
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

/// `patch` of `entry` failed to build with `status` in a build the game
/// holds: its mod turns off, and the loader's own patch says so in the log.
#[cold]
fn refused(entry: &Served, patch: &Patch, status: Status) {
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
}

/// Readies a game build of one served key for its benchmark: three patches
/// over a 64-byte base, each copying the bytes with one more into a buffer
/// the build lends, as a mod's patch does. With `of_a_mod` they are a mod's,
/// which the build runs through its profiled call, and that mod counts as
/// on without a running loader; else they are the loader's own. The closure
/// runs one build, frees its bytes and returns their length, 67 when every
/// patch ran.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_served_build(of_a_mod: bool) -> impl FnMut() -> usize {
    /// Copies its base with one byte more into a buffer the build lends.
    unsafe extern "system" fn copy(
        base: *const u8,
        base_len: u32,
        _user: *mut core::ffi::c_void,
        out: *mut *mut u8,
        out_len: *mut u32,
    ) -> Status {
        let len = base_len as usize;
        let Some(at) = lend(len + 1) else {
            return Status::REFUSED;
        };
        // SAFETY: the build passes `len` readable bytes at `base`, and `lend`
        // gave `len + 1` writable bytes at `at` apart from them.
        unsafe { core::ptr::copy_nonoverlapping(base, at, len) };
        // SAFETY: `lend` gave `len + 1` writable bytes at `at`, so the byte
        // at `len` is its last.
        unsafe { at.wrapping_add(len).write(0) };
        // SAFETY: the build passes `out` pointing at a local, writable for
        // the call.
        unsafe { out.write(at) };
        // SAFETY: as `out`, for `out_len`.
        unsafe { out_len.write(base_len + 1) };
        Status::OK
    }
    let key = ResourceKey {
        guid: enshwick_core::content::Guid([3; 16]),
        type_hash: 1,
        part: 0,
    };
    let owner = if of_a_mod {
        ModIndex::new(0)
    } else {
        ModIndex::LOADER
    };
    let patches = (0..3).map(|_| Patch::new(owner, copy, 0)).collect();
    let entry: &'static Served = Box::leak(Box::new(Served::new(key, key, patches, false)));
    let base: &'static [u8] = Box::leak(vec![7u8; 64].into_boxed_slice());
    let at = base.as_ptr().expose_provenance();
    let table = Table {
        on: |_| true,
        ..Table::game(core::slice::from_ref(entry))
    };
    move || {
        // SAFETY: `base` is leaked, so its bytes stay readable and unchanged.
        unsafe { builds::build(&table, entry, (at, base.len()), Build::Game, None) }
            .map_or(0, |built| built.len)
    }
}

#[cfg(test)]
mod tests;

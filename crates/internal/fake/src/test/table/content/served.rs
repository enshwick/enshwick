//! Served resources: what mods serve, their storage behind the shared build
//! routine the loader also runs, rebuilds, and the reads of archived and
//! served bytes.

use core::ffi::c_void;

use crate::test::ServedResource;
use crate::test::table::{Registered, put, with, with_open};
use enshwick_base::__internal::{
    Build, BuildResourceError, Built, Held, KeepBase, PatchFailure, Patches, ServedTable, View,
};
use enshwick_core::abi::{ModHandle, ResourceKey, ResourcePatchFn, Status};

pub(crate) extern "system" fn serve_resource(
    me: ModHandle,
    key: *const ResourceKey,
    base: *const ResourceKey,
    patch: Option<ResourcePatchFn>,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes `key` and `base` null or pointing at
    // keys valid for the call.
    unsafe { serve(me, key, base, None, patch, user) }
}

pub(crate) extern "system" fn rekey_resource(
    me: ModHandle,
    key: *const ResourceKey,
    from: *const ResourceKey,
    patch: Option<ResourcePatchFn>,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes `from` null or pointing at a key valid
    // for the call; `as_ref` turns away null.
    let Some(from) = (unsafe { from.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer passes `key` null or pointing at a key valid
    // for the call, which serves as its own base.
    unsafe { serve(me, key, key, Some(from), patch, user) }
}

/// Adds mod `me`'s `patch` of the key at `key` from the key at `base`; with
/// `rekey`, a re-key whose served read of that key leaves out its re-keys.
///
/// # Safety
///
/// - `key` and `base` are null or point at a `ResourceKey` readable for the
///   call.
unsafe fn serve(
    me: ModHandle,
    key: *const ResourceKey,
    base: *const ResourceKey,
    rekey: Option<ResourceKey>,
    patch: Option<ResourcePatchFn>,
    user: *mut c_void,
) -> Status {
    let Some(patch) = patch else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the caller guarantees `key` null or readable; `as_ref` turns
    // away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the caller guarantees `base` null or readable; `as_ref` turns
    // away null.
    let Some(base) = (unsafe { base.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        // As the loader: a re-key's arguments are checked once it is open.
        if rekey.is_some_and(|new| !enshwick_base::__internal::rekeys_another(&key, &new)) {
            return Status::BAD_ARGS;
        }
        if !super::declared_resource(s, me, &key, &base) {
            return Status::REFUSED;
        }
        let entry = (
            ServedResource { key, base },
            Registered::new(patch, user, me),
            rekey,
        );
        // As the loader: a change chains onto the key's maker, which goes
        // first whenever it comes.
        match s
            .content
            .served
            .iter()
            .position(|served| served.0.key == key)
        {
            Some(at) if s.content.served[at].0.base == key && base != key => {
                s.content.served.insert(at, entry);
            }
            Some(at) if base != key && s.content.served[at].0.base != base => {
                return Status::REFUSED;
            }
            _ => s.content.served.push(entry),
        }
        Status::OK
    })
}

pub(crate) extern "system" fn rebuild_resource(me: ModHandle, key: *const ResourceKey) -> Status {
    // SAFETY: the safe layer passes a valid key for the call; `as_ref` turns away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // As the loader's argument layer: a mod that is off is refused.
    if crate::test::table::base::registered::is_off(me) {
        return Status::FAULTED;
    }
    let Some(table) = with(None, |s| Some(Table::new(s.base.closed()))) else {
        return Status::MISSING;
    };
    // A rebuild the mod asked for once init ended that will build, noted
    // before it does, so a rebuild its patches ask for comes after it.
    let builds = table.sealed
        && table
            .served_by(Some(me), &key)
            .is_some_and(|served| !building(&served));
    if builds {
        with((), |s| s.content.rebuilds.push(key));
    }
    let rebuilt = enshwick_base::__internal::rebuild(&table, Some(me), &key);
    rebuilt.map_or_else(BuildResourceError::status, |()| Status::OK)
}

/// Notes that the game's builds running on this thread asked whether the
/// client shows served UI images before it did, as the loader does: they
/// keep their base and [`icons_shown`] builds them again.
pub(crate) fn note_icons_read() {
    let asking: Vec<_> = BUILDING.with_borrow(|frames| {
        frames
            .iter()
            .filter(|frame| frame.notes)
            .map(|frame| frame.key)
            .collect()
    });
    with((), |s| {
        for key in asking {
            if !s.content.served_builds.reads_icons.contains(&key) {
                s.content.served_builds.reads_icons.push(key);
            }
        }
    });
}

/// Builds again every key whose game build asked whether icons show before
/// they did, and the keys that read those, each once, as the loader does
/// once the client shows served UI images.
pub(crate) fn icons_shown() {
    let table = Table::new(true);
    let asked = with(Vec::new(), |s| s.content.served_builds.reads_icons.clone());
    let mut done = Vec::new();
    for key in asked {
        if !done.contains(&key) {
            // As the loader: a key that cannot be built again keeps its icons.
            let _kept = enshwick_base::__internal::rebuild_in(&table, key, &mut done);
        }
    }
}

/// The bytes the game holds for served `key`, when its first build kept
/// them for a rebuild, as each rebuild since left them.
pub(crate) fn held(key: &ResourceKey) -> Option<Vec<u8>> {
    with(None, |s| {
        s.content
            .served_builds
            .game
            .iter()
            .find(|built| built.key == *key)
            .and_then(|built| built.kept.as_ref())
            .map(|(_, held)| held.clone())
    })
}

pub(crate) extern "system" fn resource_read(
    _me: ModHandle,
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer's arguments meet `copy_resource`'s guarantees.
    unsafe {
        copy_resource(key, out, capacity, len, |key| {
            archived(&key).ok_or(Status::MISSING)
        })
    }
}

pub(crate) extern "system" fn resource_served(
    _me: ModHandle,
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer's arguments meet `copy_resource`'s guarantees.
    unsafe {
        copy_resource(key, out, capacity, len, |key| {
            // As the loader: no read before every patch is in.
            let table = Table::new(crate::test::table::late());
            enshwick_base::__internal::read_served(&table, &key).map_err(BuildResourceError::status)
        })
    }
}

/// The fake archive's resource `key`.
fn archived(key: &ResourceKey) -> Option<Vec<u8>> {
    with(None, |s| {
        s.host
            .content
            .resources
            .get(&(key.guid, key.type_hash, key.part))
            .cloned()
    })
}

/// Copies the bytes `read` gives for the key at `key` to `out`, writing
/// their length to `len`; [`Status::BAD_ARGS`] when `capacity` is short, and
/// the status `read` fails with.
///
/// # Safety
///
/// - `key` is null or points at a `ResourceKey` readable for the call.
/// - `len` is null or points at a writable, aligned `u32`.
/// - `out` is null or writable for `capacity` bytes, apart from `read`'s.
unsafe fn copy_resource(
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    read: impl FnOnce(ResourceKey) -> Result<Vec<u8>, Status>,
) -> Status {
    // SAFETY: the caller guarantees `key` null or readable; `as_ref` turns
    // away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let data = match read(key) {
        Ok(data) => data,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees `len` null or writable and aligned, and
    // `out` null or `capacity` writable bytes apart from `data`, which this
    // function owns.
    unsafe { enshwick_core::__fake::copy_out(&data, out, capacity, len) }
}

/// A served key whose patches run, with the buffers they took, freed when
/// its build ends.
struct Frame {
    key: ResourceKey,
    /// Whether a served read inside makes the key a reader: in the game's
    /// builds and rebuilds, not in a served read.
    notes: bool,
    /// Whether a patch read another served key as served.
    reads: bool,
    buffers: Vec<Box<[u8]>>,
}

thread_local! {
    /// The builds running on this thread, as the loader's.
    static BUILDING: core::cell::RefCell<Vec<Frame>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

/// Whether `key`'s patches run on this thread.
fn building(key: &ResourceKey) -> bool {
    BUILDING.with_borrow(|frames| frames.iter().any(|frame| frame.key == *key))
}

/// `read` of the bytes at `view` when a buffer the build running on this
/// thread lent holds them all.
fn lent<R>(view: View, read: impl FnOnce(&[u8]) -> R) -> Option<R> {
    BUILDING.with_borrow(|frames| {
        let bytes = frames.last()?.buffers.iter().find_map(|buffer| {
            let start = view.at.checked_sub(buffer.as_ptr().addr())?;
            buffer.get(start..start.checked_add(view.len as usize)?)
        })?;
        Some(read(bytes))
    })
}

/// Notes that the game's builds running on this thread read served `key`,
/// so a rebuild of it rebuilds them, as the loader does.
fn note_read(key: ResourceKey) {
    let readers: Vec<_> = BUILDING.with_borrow_mut(|frames| {
        frames
            .iter_mut()
            .filter(|frame| frame.notes)
            .map(|frame| {
                frame.reads = true;
                frame.key
            })
            .collect()
    });
    with((), |s| {
        for reader in readers {
            if !s.content.served_builds.read_by.contains(&(key, reader)) {
                s.content.served_builds.read_by.push((key, reader));
            }
        }
    });
}

pub(crate) extern "system" fn resource_buffer(
    _me: ModHandle,
    len: u32,
    out: *mut *mut u8,
) -> Status {
    let Ok(len) = usize::try_from(len) else {
        return Status::BAD_ARGS;
    };
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    let lent = BUILDING.with_borrow_mut(|frames| {
        let buffers = &mut frames.last_mut()?.buffers;
        // As the loader: bytes that do not allocate refuse the call.
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(len).ok()?;
        buffer.resize(len, 0);
        buffers.push(buffer.into_boxed_slice());
        buffers.last_mut().map(|buffer| buffer.as_mut_ptr())
    });
    match lent {
        Some(at) => {
            with((), |s| s.content.lent_buffers += 1);
            put(out, at)
        }
        None => Status::REFUSED,
    }
}

/// What the game's builds of served keys left for a rebuild, as the loader
/// keeps it.
#[derive(Debug, Default)]
pub(crate) struct ServedBuilds {
    /// The game's first build of each key it built.
    game: Vec<GameBuild>,
    /// The keys a mod asked during init to rebuild, whose first build keeps
    /// its base.
    keep_base: Vec<ResourceKey>,
    /// Served reads inside the game's builds: the key read, then the key
    /// whose build read it.
    read_by: Vec<(ResourceKey, ResourceKey)>,
    /// The keys whose game builds asked whether icons show before they
    /// did, whose first build keeps its base.
    reads_icons: Vec<ResourceKey>,
}

impl ServedBuilds {
    /// The keys whose game builds read `key`.
    fn readers(&self, key: &ResourceKey) -> Vec<ResourceKey> {
        self.read_by
            .iter()
            .filter(|(read, _)| read == key)
            .map(|(_, reader)| *reader)
            .collect()
    }
}

/// The game's first build of a served key, which stays for the process.
#[derive(Debug)]
struct GameBuild {
    key: ResourceKey,
    /// Whether the game holds bytes: built, or a changed vanilla key whose
    /// every patch failed, which reads as the archive has it.
    holds: bool,
    /// The base it was built from and the bytes the game holds, kept when a
    /// mod asked during init to rebuild the key, a patch read another
    /// served key or asked whether icons show before they did.
    kept: Option<(Vec<u8>, Vec<u8>)>,
    /// The mods whose patches built the bytes the game holds; `None` for a
    /// test's stand-in for another mod.
    by: Vec<Option<ModHandle>>,
}

/// The game's first build of the served resource `key` from `base`, by the
/// shared routine the loader runs, recorded once: the mods that built its
/// bytes, with its base and bytes when kept for a rebuild.
///
/// # Errors
///
/// As [`enshwick_base::__internal::build`].
pub(crate) fn build_first(key: &ResourceKey, base: &[u8]) -> Result<Vec<u8>, BuildResourceError> {
    // A test's game build inside a build of the key answers so and leaves no
    // record, so the test's real first build still records.
    if building(key) {
        return Err(BuildResourceError::Building);
    }
    let table = Table::new(true);
    let at = (base.as_ptr().expose_provenance(), base.len());
    // SAFETY: `base` is a slice borrowed for the call.
    let first = unsafe { enshwick_base::__internal::build_first(&table, *key, at) };
    // As the loader, which builds only served keys: the game holds nothing
    // of a key no mod serves.
    let holds = first.holds && table.find(key).is_ok();
    with((), |s| {
        // As the loader: the game builds a key once and holds it for the
        // process.
        if s.content
            .served_builds
            .game
            .iter()
            .any(|held| held.key == *key)
        {
            return;
        }
        let built = first.built.as_ref().ok();
        s.content.served_builds.game.push(GameBuild {
            key: *key,
            holds,
            kept: built
                .filter(|_| first.keep_base)
                .map(|built| (base.to_vec(), built.bytes.clone())),
            by: built.map_or_else(Vec::new, |built| built.by.clone()),
        });
    });
    first.built.map(|built| built.bytes)
}

/// The name the loader gives served resource `key`, from the fake's type
/// hashes.
fn served_name(key: &ResourceKey) -> String {
    with(format!("resource {}", key.guid), |s| {
        enshwick_core::resources::served_name(key, |id| s.host.base.type_hashes.get(id).copied())
    })
}

/// A patch of mod `owner` failed to build `key` with `status` in a build
/// the game holds: the mod turns off with the loader's reason, once.
fn refused(key: &ResourceKey, owner: Option<ModHandle>, status: Status) {
    let Some(owner) = owner else {
        return;
    };
    if crate::test::table::base::registered::is_off(owner) {
        return;
    }
    let reason = format!("served {} failed to build ({})", served_name(key), status.0);
    crate::test::table::base::mods::loader_turns_off(owner, reason);
}

/// Turns off the patch at `at` in `served` of `key`, whose builder panicked,
/// and has its mod report the resource off, once, as the loader does: a
/// rebuild runs a patch already off again.
fn panicked(key: &ResourceKey, at: usize, owner: Option<ModHandle>) {
    let name = format!("served {}", served_name(key));
    let line = format!(
        "feature {name} off: {}",
        enshwick_core::resources::BUILDER_PANICKED
    );
    with((), |s| {
        if s.content.served_off.contains(&at) {
            return;
        }
        s.content.served_off.push(at);
        match s.base.features.iter_mut().find(|(have, _)| *have == name) {
            Some(kept) => kept.1 = Some(enshwick_core::resources::BUILDER_PANICKED.to_owned()),
            None => s.base.features.push((
                name,
                Some(enshwick_core::resources::BUILDER_PANICKED.to_owned()),
            )),
        }
        if let Some(owner) = owner {
            s.base
                .sent
                .push((owner, enshwick_core::abi::Level::WARN, line.clone()));
        }
        s.base.logs.push((enshwick_core::abi::Level::WARN, line));
    });
}

/// Marks a key's patches running on this thread until dropped, unwinding
/// included, as the loader's.
struct Building;

impl Building {
    /// Begins a build of `key`; `notes` for the game's builds and rebuilds,
    /// whose served reads make `key` a reader.
    fn enter(key: ResourceKey, notes: bool) -> Self {
        BUILDING.with_borrow_mut(|building| {
            building.push(Frame {
                key,
                notes,
                reads: false,
                buffers: Vec::new(),
            });
        });
        Self
    }
}

impl Drop for Building {
    fn drop(&mut self) {
        let freed = BUILDING
            .with_borrow_mut(Vec::pop)
            .map_or(0, |frame| frame.buffers.len());
        with((), |s| {
            s.content.lent_buffers = s.content.lent_buffers.saturating_sub(freed);
        });
    }
}

/// The fake's served keys as the shared build routine reads them; `sealed`
/// as the loader's registrations are.
struct Table {
    sealed: bool,
}

impl Table {
    const fn new(sealed: bool) -> Self {
        Self { sealed }
    }
}

/// One patch of a served key: where it sits in the fake's list, and the key
/// whose data it takes when it is a re-key.
#[derive(Clone, Copy)]
struct Patch {
    at: usize,
    registered: Registered<ResourcePatchFn>,
    rekey: Option<ResourceKey>,
}

// SAFETY: `apply` answers an output only when `lent` finds it in a buffer of
// the running frame, which `Building` frees only when `leave` drops it.
unsafe impl Patches for Table {
    type Key = ResourceKey;
    type Owner = Option<ModHandle>;
    type Patch = Patch;
    type Frame = Building;
    type Bytes = Vec<u8>;

    fn building(&self, key: ResourceKey) -> bool {
        building(&key)
    }

    fn patches(&self, key: ResourceKey) -> impl Iterator<Item = Patch> {
        let patches: Vec<Patch> = with(Vec::new(), |s| {
            s.content
                .served
                .iter()
                .enumerate()
                .filter(|(_, served)| served.0.key == key)
                .map(|(at, served)| Patch {
                    at,
                    registered: served.1,
                    rekey: served.2,
                })
                .collect()
        });
        patches.into_iter()
    }

    fn owner(&self, patch: &Patch) -> Option<ModHandle> {
        patch.registered.owner()
    }

    fn on(&self, patch: &Patch) -> bool {
        // As the loader: read when the patch's turn comes, so an earlier
        // patch that turned its mod off skips the mod's later ones.
        with(false, |s| {
            !s.content.served_off.contains(&patch.at) && !patch.registered.owner_off(&s.base.off)
        })
    }

    fn rekey(&self, patch: &Patch) -> Option<ResourceKey> {
        patch.rekey
    }

    fn enter(&self, key: ResourceKey, build: Build) -> Building {
        Building::enter(key, build == Build::Game)
    }

    unsafe fn apply(&self, patch: &Patch, input: View, rebuild: bool) -> (Status, Option<View>) {
        let mut out = core::ptr::null_mut::<u8>();
        let mut out_len = 0;
        let base = core::ptr::with_exposed_provenance::<u8>(input.at);
        // SAFETY: the caller guarantees `input` readable for the call; the
        // outs are locals and `user` is the pointer the mod served with.
        let call = |apply: ResourcePatchFn, user| unsafe {
            apply(base, input.len, user, &raw mut out, &raw mut out_len)
        };
        let status = if rebuild {
            patch.registered.run_even_off(call)
        } else {
            patch.registered.run(call)
        };
        let output = View {
            at: out.expose_provenance(),
            len: out_len,
        };
        (status, lent(output, |_| ()).map(|()| output))
    }

    fn failed(&self, key: ResourceKey, patch: &Patch, why: PatchFailure) {
        let owner = patch.registered.owner();
        match why {
            PatchFailure::Panicked => panicked(&key, patch.at, owner),
            PatchFailure::Refused(status) => refused(&key, owner, status),
        }
    }

    fn leave(&self, frame: Building, last: Option<View>) -> (bool, Option<Vec<u8>>) {
        let bytes = last.and_then(|last| lent(last, <[u8]>::to_vec));
        let reads = BUILDING.with_borrow(|frames| frames.last().is_some_and(|frame| frame.reads));
        drop(frame);
        (reads, bytes)
    }
}

// SAFETY: `base` answers the bytes of the kept `Vec`, which nothing changes
// while it lives.
unsafe impl ServedTable for Table {
    type Kept = Vec<u8>;

    fn sealed(&self) -> bool {
        self.sealed
    }

    fn find(&self, key: &ResourceKey) -> Result<ResourceKey, ResourceKey> {
        let served = with(false, |s| {
            s.content.served.iter().any(|served| served.0.key == *key)
        });
        if served { Ok(*key) } else { Err(*key) }
    }

    fn served_by(&self, owner: Option<ModHandle>, key: &ResourceKey) -> Option<ResourceKey> {
        with(false, |s| {
            s.content
                .served
                .iter()
                .any(|served| served.0.key == *key && served.1.owner() == owner)
        })
        .then_some(*key)
    }

    fn keys(&self, key: ResourceKey) -> (ResourceKey, ResourceKey) {
        let base = with(None, |s| {
            s.content
                .served
                .iter()
                .find(|served| served.0.key == key)
                .map(|served| served.0.base)
        });
        (key, base.unwrap_or(key))
    }

    fn archived(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        archived(key)
    }

    fn keep_base(&self, key: ResourceKey) {
        with((), |s| {
            s.content.rebuilds.push(key);
            if !s.content.served_builds.keep_base.contains(&key) {
                s.content.served_builds.keep_base.push(key);
            }
        });
    }

    fn keeps_base(&self, key: ResourceKey) -> KeepBase {
        with(KeepBase::default(), |s| KeepBase {
            asked: s.content.served_builds.keep_base.contains(&key),
            asked_icons: s.content.served_builds.reads_icons.contains(&key),
            ..KeepBase::default()
        })
    }

    fn note_read(&self, key: ResourceKey) {
        note_read(key);
    }

    fn held(&self, key: ResourceKey) -> Option<Held<Vec<u8>, Option<ModHandle>>> {
        with(None, |s| {
            let built = s
                .content
                .served_builds
                .game
                .iter()
                .find(|built| built.key == key && built.holds)?;
            Some(Held {
                base: built.kept.as_ref().map(|(base, _)| base.clone()),
                len: built.kept.as_ref().map_or(0, |(_, held)| held.len()),
                by: built.by.clone(),
            })
        })
    }

    fn base(&self, kept: &Vec<u8>) -> (usize, usize) {
        (kept.as_ptr().expose_provenance(), kept.len())
    }

    fn write(
        &self,
        key: ResourceKey,
        _kept: Vec<u8>,
        built: Built<Vec<u8>, Option<ModHandle>>,
    ) -> usize {
        with(0, |s| {
            let Some((_, held)) = s
                .content
                .served_builds
                .game
                .iter_mut()
                .find(|game| game.key == key)
                .and_then(|game| game.kept.as_mut())
            else {
                return 0;
            };
            let changed = built
                .bytes
                .iter()
                .zip(held.iter())
                .filter(|(new, old)| new != old)
                .count();
            *held = built.bytes;
            changed
        })
    }

    fn readers(&self, key: ResourceKey) -> Vec<ResourceKey> {
        with(Vec::new(), |s| s.content.served_builds.readers(&key))
    }

    fn to_vec(&self, built: Built<Vec<u8>, Option<ModHandle>>) -> Vec<u8> {
        built.bytes
    }
}

#[cfg(test)]
mod tests {
    use enshwick_core::abi::{Guid, ResourceKey};

    use super::{BuildResourceError, Building, build_first};
    use crate::test::FakeHost;
    use crate::test::table::with;

    const KEY: ResourceKey = ResourceKey {
        guid: Guid([0x1f; 16]),
        type_hash: 1,
        part: 0,
    };

    #[test]
    fn a_game_build_inside_a_build_of_its_key_is_refused_and_leaves_no_record() {
        let _loader = FakeHost::new().install();
        let frame = Building::enter(KEY, false);
        assert_eq!(build_first(&KEY, &[1]), Err(BuildResourceError::Building));
        drop(frame);
        assert!(with(false, |s| s.content.served_builds.game.is_empty()));
        assert_eq!(build_first(&KEY, &[1]), Err(BuildResourceError::NotBuilt));
        assert!(with(false, |s| s.content.served_builds.game.len() == 1));
    }
}

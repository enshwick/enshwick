//! Served resources: what mods serve, the builds of their patches as the
//! loader runs them, rebuilds, and the reads of archived and served bytes.

use core::ffi::c_void;

use crate::test::ServedResource;
use crate::test::table::{Registered, put, with, with_open};
use enshwick_base::__internal::{BuildResourceError, KeepBase, patch_runs};
use enshwick_core::abi::{ModHandle, ResourceKey, ResourcePatchFn, Status};

pub(crate) extern "system" fn serve_resource(
    me: ModHandle,
    key: *const ResourceKey,
    base: *const ResourceKey,
    patch: Option<ResourcePatchFn>,
    user: *mut c_void,
) -> Status {
    let Some(patch) = patch else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer passes valid keys for the call; `as_ref` turns away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer passes `base` null or pointing at a key valid
    // for the call; `as_ref` turns away null.
    let Some(base) = (unsafe { base.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with_open(|s| {
        if !super::declared_resource(s, me, &key, &base) {
            return Status::REFUSED;
        }
        let entry = (
            ServedResource { key, base },
            Registered::new(patch, user, me),
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
    let closed = with(None, |s| {
        // As the loader: only a mod serving the key rebuilds it.
        let serves = s
            .content
            .served
            .iter()
            .any(|served| served.0.key == key && served.1.owner() == Some(me));
        Some(if serves {
            Ok(s.base.closed())
        } else {
            Err(BuildResourceError::NotServed)
        })
    });
    match closed {
        None => Status::MISSING,
        Some(Err(why)) => why.status(),
        // As the loader during init: the key's first build keeps its base.
        Some(Ok(false)) => with(Status::MISSING, |s| {
            s.content.rebuilds.push(key);
            if !s.content.served_builds.keep_base.contains(&key) {
                s.content.served_builds.keep_base.push(key);
            }
            Status::OK
        }),
        Some(Ok(true)) => rebuild(key).map_or_else(BuildResourceError::status, |()| Status::OK),
    }
}

/// Builds `key` again, then the keys whose builds read it when it changed,
/// as the loader's rebuild once init ended: [`BuildResourceError::Building`] while
/// its patches run on this thread, as a patch rebuilding its own key would
/// wait on its build.
fn rebuild(key: ResourceKey) -> Result<(), BuildResourceError> {
    if building(&key) {
        return Err(BuildResourceError::Building);
    }
    with((), |s| s.content.rebuilds.push(key));
    if rebuild_one(&key)? != Some(0) {
        rebuild_readers(key, &mut vec![key]);
    }
    Ok(())
}

/// Builds `key` again from the base its game build kept, with the patches
/// of every mod that built the bytes the game holds, on or off, and takes
/// the result as the bytes the game holds, as the loader's rebuild: how many
/// bytes changed, `None` when the game holds none.
fn rebuild_one(key: &ResourceKey) -> Result<Option<usize>, BuildResourceError> {
    let found = with(None, |s| {
        s.content
            .served_builds
            .game
            .iter()
            .find(|built| built.key == *key)
            .map(|built| (built.holds, built.kept.clone(), built.by.clone()))
    });
    let Some((true, kept, by)) = found else {
        return Ok(None);
    };
    let Some((base, held)) = kept else {
        return Err(BuildResourceError::NoBase);
    };
    let rebuilt = build_resource(key, &base, Build::Rebuild(&by))?;
    if rebuilt.len() != held.len() {
        return Err(BuildResourceError::SizeChanged);
    }
    let changed = rebuilt
        .iter()
        .zip(&held)
        .filter(|(new, old)| new != old)
        .count();
    with((), |s| {
        if let Some((_, held)) = s
            .content
            .served_builds
            .game
            .iter_mut()
            .find(|built| built.key == *key)
            .and_then(|built| built.kept.as_mut())
        {
            *held = rebuilt;
        }
    });
    Ok(Some(changed))
}

/// Rebuilds the keys whose game builds read `key`, which changed, and in
/// turn those that read a key that changed, each once and none of `done`,
/// as the loader does; a key whose build runs on this thread is left.
fn rebuild_readers(key: ResourceKey, done: &mut Vec<ResourceKey>) {
    let readers = |key: &ResourceKey| with(Vec::new(), |s| s.content.served_builds.readers(key));
    let mut next = readers(&key);
    while let Some(reader) = next.pop() {
        if done.contains(&reader) || building(&reader) {
            continue;
        }
        done.push(reader);
        if rebuild_one(&reader).is_ok_and(|changed| changed != Some(0)) {
            next.extend(readers(&reader));
        }
    }
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
    let asked = with(Vec::new(), |s| s.content.served_builds.reads_icons.clone());
    let mut done = Vec::new();
    for key in asked {
        if done.contains(&key) || building(&key) {
            continue;
        }
        done.push(key);
        if rebuild_one(&key).is_ok_and(|changed| changed != Some(0)) {
            rebuild_readers(key, &mut done);
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
    unsafe { copy_resource(key, out, capacity, len, |key| archived(&key)) }
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
            if !crate::test::table::late() {
                return Err(BuildResourceError::Unsealed);
            }
            let Some(base) = with(None, |s| {
                s.content
                    .served
                    .iter()
                    .find(|served| served.0.key == key)
                    .map(|served| served.0.base)
            }) else {
                return archived(&key);
            };
            note_read(key);
            let archived = archived(&base)?;
            match build_resource(&key, &archived, Build::Scratch) {
                // As the loader: a key whose every patch failed reads as
                // the archive has it.
                Err(BuildResourceError::NotBuilt) if key == base => Ok(archived),
                Err(BuildResourceError::NotBuilt) => self::archived(&key),
                built => built,
            }
        })
    }
}

/// The fake archive's resource `key`.
fn archived(key: &ResourceKey) -> Result<Vec<u8>, BuildResourceError> {
    with(None, |s| {
        s.host
            .content
            .resources
            .get(&(key.guid, key.type_hash, key.part))
            .cloned()
    })
    .ok_or(BuildResourceError::NotInArchive)
}

/// Copies the bytes `read` gives for the key at `key` to `out`, writing
/// their length to `len`; [`Status::BAD_ARGS`] when `capacity` is short, and
/// the status of `read`'s error.
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
    read: impl FnOnce(ResourceKey) -> Result<Vec<u8>, BuildResourceError>,
) -> Status {
    // SAFETY: the caller guarantees `key` null or readable; `as_ref` turns
    // away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let data = match read(key) {
        Ok(data) => data,
        Err(why) => return why.status(),
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

/// Who a build of a served resource is for, as the loader tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Build<'a> {
    /// The game's first build, whose bytes the game holds: the patches of
    /// the mods on run, and one that fails turns its mod off.
    Game,
    /// A rebuild of the bytes the game holds: every patch of the mods that
    /// built them runs, on or off, one whose builder panicked since
    /// included, as the game holds what each added; one that fails turns
    /// its mod off.
    Rebuild(&'a [Option<ModHandle>]),
    /// A mod's served read, kept nowhere: a patch that fails is skipped and
    /// its mod stays on.
    Scratch,
}

impl<'a> Build<'a> {
    /// The mods whose patches a rebuild runs; `None` for another build.
    const fn rebuilt_by(self) -> Option<&'a [Option<ModHandle>]> {
        match self {
            Self::Rebuild(by) => Some(by),
            Self::Game | Self::Scratch => None,
        }
    }
}

/// Builds the served resource `key` from `base` with the patches `kind`
/// runs, in order: [`BuildResourceError::NotBuilt`] when none built. As the loader,
/// [`BuildResourceError::Building`] for a key whose patches already run on this thread;
/// a patch that refuses or hands back bytes it was not lent turns its mod
/// off in the game's build or a rebuild and is skipped in a scratch one; a
/// patch whose builder panicked ([`Status::FAULTED`]) runs again only in a
/// rebuild: its mod reports the resource as a feature off; and the game's
/// first build of a key records the mods that built its bytes, with its base
/// and bytes when kept for a rebuild, while a later one leaves that record.
pub(crate) fn build_resource(
    key: &ResourceKey,
    base: &[u8],
    kind: Build<'_>,
) -> Result<Vec<u8>, BuildResourceError> {
    if building(key) {
        return Err(BuildResourceError::Building);
    }
    let patches: Vec<_> = with(Vec::new(), |s| {
        s.content
            .served
            .iter()
            .enumerate()
            .filter(|(at, served)| {
                served.0.key == *key
                    && patch_runs(kind.rebuilt_by(), &served.1.owner(), || {
                        !s.content.served_off.contains(at) && !served.1.owner_off(&s.base.off)
                    })
            })
            .map(|(at, served)| (at, served.1))
            .collect()
    });
    let building = Building::enter(*key, kind != Build::Scratch);
    let mut bytes = base.to_vec();
    let Ok(base_len) = u32::try_from(bytes.len()) else {
        return Err(BuildResourceError::NotBuilt);
    };
    let mut len = base_len;
    let mut by = Vec::new();
    for (at, patch) in patches {
        let mut out = core::ptr::null_mut();
        let mut out_len = 0;
        // SAFETY: `bytes` is readable for the call, the outs are valid, and
        // `user` is the pointer the mod served with.
        let apply = |patch: ResourcePatchFn, user| unsafe {
            patch(bytes.as_ptr(), len, user, &raw mut out, &raw mut out_len)
        };
        let status = match kind {
            Build::Rebuild(_) => patch.run_even_off(apply),
            Build::Game | Build::Scratch => patch.run(apply),
        };
        if status == Status::FAULTED {
            panicked(key, at, patch.owner());
            continue;
        }
        // As the loader: an output outside the buffers this build lent, null
        // included, fails the patch.
        let output = BUILDING.with_borrow(|frames| {
            frames.last().and_then(|frame| {
                frame.buffers.iter().find_map(|buffer| {
                    let start = out.addr().checked_sub(buffer.as_ptr().addr())?;
                    buffer
                        .get(start..start.checked_add(out_len as usize)?)
                        .map(<[u8]>::to_vec)
                })
            })
        });
        match (status.is_ok(), output) {
            (true, Some(output)) => {
                bytes = output;
                len = out_len;
                by.push(patch.owner());
            }
            // As the loader: an output it did not lend is a bad one.
            (true, None) => refused(key, kind, patch.owner(), Status::BAD_ARGS),
            (false, _) => refused(key, kind, patch.owner(), status),
        }
    }
    let reads = building.leave();
    let built = (!by.is_empty()).then_some(bytes);
    if kind == Build::Game {
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
            let vanilla = s
                .content
                .served
                .iter()
                .any(|served| served.0.key == *key && served.0.base == *key);
            let builds = &s.content.served_builds;
            let keeps = KeepBase {
                asked: builds.keep_base.contains(key),
                read_served: reads,
                asked_icons: builds.reads_icons.contains(key),
            }
            .keeps();
            s.content.served_builds.game.push(GameBuild {
                key: *key,
                holds: built.is_some() || vanilla,
                kept: built
                    .as_ref()
                    .filter(|_| keeps)
                    .map(|bytes| (base.to_vec(), bytes.clone())),
                by,
            });
        });
    }
    built.ok_or(BuildResourceError::NotBuilt)
}

/// The name the loader gives served resource `key`, from the fake's type
/// hashes.
fn served_name(key: &ResourceKey) -> String {
    with(format!("resource {}", key.guid), |s| {
        enshwick_core::resources::served_name(key, |id| s.host.base.type_hashes.get(id).copied())
    })
}

/// A patch of mod `owner` failed to build `key` with `status`: in a build
/// the game holds the mod turns off with the loader's reason, once; a
/// scratch build skips it.
fn refused(key: &ResourceKey, kind: Build<'_>, owner: Option<ModHandle>, status: Status) {
    let Some(owner) = owner.filter(|_| kind != Build::Scratch) else {
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

    /// Ends the build: whether a patch of it read another served key as
    /// served.
    fn leave(self) -> bool {
        let reads = BUILDING.with_borrow(|frames| frames.last().is_some_and(|frame| frame.reads));
        drop(self);
        reads
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

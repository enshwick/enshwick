//! The builds of a served resource, as the loader and the fake host both run
//! them: which patches a build runs and in what order, what a failed patch
//! costs, when the game's first build keeps its base, how a rebuild reaches
//! the keys that read the one rebuilt, and what each answers a mod.
//!
//! Each side keeps its own storage behind [`Patches`] and [`ServedTable`]
//! and makes no decision of its own, so the fake cannot drift from the
//! loader.

use crate::abi::{ResourceKey, Status};

/// Why a served resource was not rebuilt or read, as the loader and the fake
/// answer a mod. Exhaustive, so both handle every outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BuildResourceError {
    /// The mod serves no such key.
    #[error("the mod serves no such resource")]
    NotServed,
    /// The key's patches run on this thread, which would wait on itself.
    #[error("the resource's patches run on this thread")]
    Building,
    /// No base was kept to build the key again from.
    #[error("no base was kept for the resource")]
    NoBase,
    /// No patch built the key.
    #[error("no patch built the resource")]
    NotBuilt,
    /// The rebuild made another size than the game holds.
    #[error("the rebuild made another size than the game holds")]
    SizeChanged,
    /// Asked before the registrations sealed.
    #[error("asked before the resources sealed")]
    Unsealed,
    /// The game's archive has no such resource.
    #[error("the archive has no such resource")]
    NotInArchive,
}

impl BuildResourceError {
    /// The status a mod's call gets: a key the mod does not serve is a bad
    /// argument, a build on this thread says so, a missing base, build or
    /// archive entry is missing, and a changed size or a read before the
    /// seal is refused.
    #[must_use]
    pub const fn status(self) -> Status {
        match self {
            Self::NotServed => Status::BAD_ARGS,
            Self::Building => Status::BUILDING,
            Self::NoBase | Self::NotBuilt | Self::NotInArchive => Status::MISSING,
            Self::SizeChanged | Self::Unsealed => Status::REFUSED,
        }
    }
}

/// Who a build of a served resource is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    /// The game's first build or a rebuild, whose bytes the game holds: a
    /// patch that fails turns its mod off, and a served read inside makes
    /// the key a reader of the key read.
    Game,
    /// A mod's served read, kept nowhere: a patch that fails is skipped with
    /// its mod left on.
    Scratch,
    /// A re-key's served read of the key it takes the data of: as
    /// [`Build::Scratch`], without that key's own re-keys, so the re-keys of
    /// every mod apply at once.
    Unkeyed,
}

/// Bytes a build reads: where they lie and how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// Their address, exposed so the side that lent them reads them again.
    pub at: usize,
    /// How many, as a patch gets them.
    pub len: u32,
}

/// Why a patch did not build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchFailure {
    /// Its builder panicked: the patch turns off for the process, and its
    /// mod reports the resource as a feature off.
    Panicked,
    /// It answered this status, or handed back bytes no buffer of the
    /// build holds ([`Status::BAD_ARGS`]): in a build the game holds, its
    /// mod turns off.
    Refused(Status),
}

/// What a side logs of a build that made nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unbuilt {
    /// The base is over 4 GiB, more than a patch can take.
    TooLarge,
    /// A rebuild made `built` bytes where the game holds `held`.
    SizeChanged {
        /// The rebuild's length.
        built: usize,
        /// The length the game holds.
        held: usize,
    },
}

/// A finished build: its bytes, the mods whose patches made them, in order,
/// and whether a patch read another served key.
#[derive(Debug)]
pub struct Built<B, O> {
    /// The bytes, owned by the side that lent their buffer.
    pub bytes: B,
    /// How many.
    pub len: usize,
    /// The mods whose patches built them.
    pub by: Vec<O>,
    /// Whether a patch read another served key as served.
    pub reads: bool,
}

/// What the game holds of a served key, for a rebuild.
#[derive(Debug)]
pub struct Held<K, O> {
    /// The base its first build kept; `None` when it kept none.
    pub base: Option<K>,
    /// The length of the bytes the game holds.
    pub len: usize,
    /// The mods whose patches built them.
    pub by: Vec<O>,
}

/// The game's first build of a served key, as its side records it.
#[derive(Debug)]
pub struct FirstBuild<B, O> {
    /// The build, or why there is none.
    pub built: Result<Built<B, O>, BuildResourceError>,
    /// Whether the build keeps a copy of its base for a rebuild.
    pub keep_base: bool,
    /// Whether the game holds bytes for the key: built, or a changed vanilla
    /// key whose every patch failed, which reads as the archive has it.
    pub holds: bool,
}

/// The patches of a served key and one build of them, as a side runs them.
///
/// # Safety
///
/// An output [`apply`](Self::apply) answers lies in a buffer of the build
/// running on this thread, readable for its length and unchanged until
/// [`leave`](Self::leave) ends that build.
pub unsafe trait Patches {
    /// A served key as the side finds it.
    type Key: Copy;
    /// A mod as the side names it.
    type Owner: Copy + PartialEq;
    /// One patch of a key.
    type Patch;
    /// A build running on this thread, which ends when dropped.
    type Frame;
    /// The bytes a finished build owns.
    type Bytes;

    /// Whether `key`'s patches run on this thread.
    fn building(&self, key: Self::Key) -> bool;

    /// The patches of `key`, in the order they build.
    fn patches(&self, key: Self::Key) -> impl Iterator<Item = Self::Patch>;

    /// The mod `patch` is of.
    fn owner(&self, patch: &Self::Patch) -> Self::Owner;

    /// Whether `patch` is still on and its mod too.
    fn on(&self, patch: &Self::Patch) -> bool;

    /// The key whose data `patch` serves its key with, when it is a re-key.
    fn rekey(&self, patch: &Self::Patch) -> Option<ResourceKey>;

    /// Begins a `build` of `key` on this thread; a served read inside a
    /// [`Build::Game`] build makes `key` a reader of the key read.
    fn enter(&self, key: Self::Key, build: Build) -> Self::Frame;

    /// Runs `patch` over `input`, as a `rebuild` runs a patch that is off; its
    /// status, and its output while a buffer of this build holds it.
    ///
    /// # Safety
    ///
    /// `input` is readable for `input.len` bytes, unchanged for the call.
    unsafe fn apply(
        &self,
        patch: &Self::Patch,
        input: View,
        rebuild: bool,
    ) -> (Status, Option<View>);

    /// Records that `patch` of `key` failed, as `why` says.
    fn failed(&self, key: Self::Key, patch: &Self::Patch, why: PatchFailure);

    /// Ends the build in `frame`: whether a patch read another served key,
    /// and the bytes at `last`, the last output, taken from its buffer.
    fn leave(&self, frame: Self::Frame, last: Option<View>) -> (bool, Option<Self::Bytes>);

    /// Logs a build of `key` that made nothing, when the side logs.
    fn unbuilt(&self, key: Self::Key, why: Unbuilt) {
        let _ = (key, why);
    }
}

/// The served keys of a side: what mods serve, what the game holds and who
/// reads whom.
///
/// # Safety
///
/// The bytes [`base`](Self::base) answers for a kept base are readable and
/// unchanged while that kept base lives.
pub unsafe trait ServedTable: Patches {
    /// What a rebuild holds while it builds: the kept base, and whatever
    /// keeps two rebuilds of a key apart.
    type Kept;

    /// Whether the registrations sealed.
    fn sealed(&self) -> bool;

    /// The served key a read of `key` builds, or the key the archive reads
    /// in its place.
    fn find(&self, key: &ResourceKey) -> Result<Self::Key, ResourceKey>;

    /// The served key `key` whose patches include one of `owner`.
    fn served_by(&self, owner: Self::Owner, key: &ResourceKey) -> Option<Self::Key>;

    /// The resource `key` serves and the base it builds from.
    fn keys(&self, key: Self::Key) -> (ResourceKey, ResourceKey);

    /// The archive's resource `key`.
    fn archived(&self, key: &ResourceKey) -> Option<Vec<u8>>;

    /// Notes during init that `key`'s first build keeps its base.
    fn keep_base(&self, key: Self::Key);

    /// Why `key`'s first build keeps its base, besides a patch reading
    /// another served key; read once the build ran.
    fn keeps_base(&self, key: Self::Key) -> KeepBase;

    /// Notes that the game's builds running on this thread read `key`.
    fn note_read(&self, key: Self::Key);

    /// What the game holds of `key`; `None` before its first build.
    fn held(&self, key: Self::Key) -> Option<Held<Self::Kept, Self::Owner>>;

    /// Where the kept base lies, and its length.
    fn base(&self, kept: &Self::Kept) -> (usize, usize);

    /// Writes the rebuild `built` of `key` over the bytes the game holds;
    /// how many bytes changed.
    fn write(
        &self,
        key: Self::Key,
        kept: Self::Kept,
        built: Built<Self::Bytes, Self::Owner>,
    ) -> usize;

    /// The keys whose game builds read `key`.
    fn readers(&self, key: Self::Key) -> Vec<Self::Key>;

    /// The bytes of `built`, owned.
    fn to_vec(&self, built: Built<Self::Bytes, Self::Owner>) -> Vec<u8>;
}

/// Whether the patch of `owner` runs in a build of its served key.
///
/// In a rebuild, whose `rebuild` names the mods that built the bytes the
/// game holds, a patch of one of them runs, on or off, since the game holds
/// what each added; in the game's first build and a served read, a patch
/// still on of a mod still on runs, as `on` answers.
pub fn patch_runs<M: PartialEq>(
    rebuild: Option<&[M]>,
    owner: &M,
    on: impl FnOnce() -> bool,
) -> bool {
    match rebuild {
        Some(by) => by.contains(owner),
        None => on(),
    }
}

/// The reasons the game's first build of a served key keeps a copy of its
/// base for a rebuild; any one is enough.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeepBase {
    /// A mod asked during init to rebuild the key.
    pub asked: bool,
    /// A patch read another served key, which a rebuild of that key builds
    /// again.
    pub read_served: bool,
    /// The build asked whether the client shows served UI images before
    /// they did.
    pub asked_icons: bool,
}

impl KeepBase {
    /// Whether the first build keeps its base.
    #[must_use]
    pub const fn keeps(self) -> bool {
        self.asked || self.read_served || self.asked_icons
    }
}

/// Whether a re-key of `key` to `new` can take effect, as both sides check
/// it: `new` is another resource of `key`'s type.
#[must_use]
pub fn rekeys_another(key: &ResourceKey, new: &ResourceKey) -> bool {
    key != new && key.type_hash == new.type_hash
}

std::thread_local! {
    /// The key whose data the re-key patch running innermost on this thread
    /// takes; `None` when the patch running there is no re-key.
    static REKEYING: core::cell::Cell<Option<ResourceKey>> = const { core::cell::Cell::new(None) };
}

/// Marks the patch running on this thread as a re-key, or as none, while it
/// lives, holding the mark of the patch around it to put back.
struct Rekeying(Option<ResourceKey>);

impl Rekeying {
    fn start(rekey: Option<ResourceKey>) -> Self {
        Self(REKEYING.replace(rekey))
    }
}

impl Drop for Rekeying {
    fn drop(&mut self) {
        REKEYING.set(self.0);
    }
}

/// Builds `key` for `kind` from the `base` bytes, where they lie and how
/// many, each patch on the output of the one before.
///
/// The patches still on of the mods still on run; for a rebuild, which
/// `only` names the mods that built the bytes the game holds, each patch of
/// those mods runs, on or off, since the game holds what each added; a
/// [`Build::Unkeyed`] build leaves out the key's re-keys. A
/// panicked patch is reported in every build; one that refuses or hands back
/// bytes it was not lent fails in a build the game holds and is skipped in a
/// scratch one. The game's first build runs even while `key`'s patches run
/// on this thread, as the game asked for it; the callers that would wait on
/// that build refuse it themselves.
///
/// # Errors
///
/// [`BuildResourceError::NotBuilt`] when no patch built or the base is over
/// 4 GiB.
///
/// # Safety
///
/// - The `len` bytes at `at` are readable and unchanged for the call, unless
///   `len` is over 4 GiB, which is refused before any read.
pub unsafe fn build<T: Patches>(
    table: &T,
    key: T::Key,
    (at, len): (usize, usize),
    kind: Build,
    only: Option<&[T::Owner]>,
) -> Result<Built<T::Bytes, T::Owner>, BuildResourceError> {
    let Ok(len) = u32::try_from(len) else {
        table.unbuilt(key, Unbuilt::TooLarge);
        return Err(BuildResourceError::NotBuilt);
    };
    let rebuild = only.is_some();
    let frame = table.enter(key, kind);
    let mut current = View { at, len };
    let mut by = Vec::new();
    for patch in table.patches(key) {
        let owner = table.owner(&patch);
        let rekey = table.rekey(&patch);
        if !patch_runs(only, &owner, || table.on(&patch))
            || (kind == Build::Unkeyed && rekey.is_some())
        {
            continue;
        }
        let running = Rekeying::start(rekey);
        // SAFETY: `current` is the base, readable for the call as the caller
        // guarantees, or an output `apply` answered in this build, readable
        // until `leave` as `Patches` guarantees.
        let (status, output) = unsafe { table.apply(&patch, current, rebuild) };
        drop(running);
        if status == Status::FAULTED {
            table.failed(key, &patch, PatchFailure::Panicked);
        } else if let (true, Some(output)) = (status == Status::OK, output) {
            current = output;
            by.push(owner);
        } else if kind == Build::Game {
            let status = if status == Status::OK {
                Status::BAD_ARGS
            } else {
                status
            };
            table.failed(key, &patch, PatchFailure::Refused(status));
        }
    }
    let last = (!by.is_empty()).then_some(current);
    let (reads, bytes) = table.leave(frame, last);
    Ok(Built {
        bytes: bytes.ok_or(BuildResourceError::NotBuilt)?,
        len: current.len as usize,
        by,
        reads,
    })
}

/// The game's first build of `key` from its `base` bytes: whether it keeps
/// its base, decided once the build ran, and whether the game holds bytes.
///
/// # Safety
///
/// - `base` meets [`build`]'s guarantees.
pub unsafe fn build_first<T: ServedTable>(
    table: &T,
    key: T::Key,
    base: (usize, usize),
) -> FirstBuild<T::Bytes, T::Owner> {
    // SAFETY: the caller guarantees `base` as `build` needs it.
    let built = unsafe { build(table, key, base, Build::Game, None) };
    let keep_base = built.as_ref().is_ok_and(|built| {
        KeepBase {
            read_served: built.reads,
            ..table.keeps_base(key)
        }
        .keeps()
    });
    let (served, from) = table.keys(key);
    FirstBuild {
        holds: built.is_ok() || served == from,
        keep_base,
        built,
    }
}

/// For mod `owner`, which serves `key`: during init, keeps its base when it
/// is first built; afterwards, builds it again and the keys that read it.
///
/// # Errors
///
/// [`BuildResourceError::NotServed`] when `owner` serves no such key;
/// otherwise as [`rebuild_in`].
pub fn rebuild<T: ServedTable>(
    table: &T,
    owner: T::Owner,
    key: &ResourceKey,
) -> Result<(), BuildResourceError> {
    let served = table
        .served_by(owner, key)
        .ok_or(BuildResourceError::NotServed)?;
    if !table.sealed() {
        table.keep_base(served);
        return Ok(());
    }
    rebuild_in(table, served, &mut Vec::new())
}

/// Builds `key` again over the bytes the game holds, then the keys whose
/// builds read it.
///
/// The readers follow when its served value may have changed: it changed, or
/// the game never built it, so only served reads saw it. One step of a pass
/// that builds each key once: the keys built go into `done`, and a reader
/// already in it is left.
///
/// # Errors
///
/// [`BuildResourceError::Building`] while `key`'s patches run on this
/// thread, as a patch rebuilding its own key would wait on its own build;
/// [`BuildResourceError::NoBase`] when its first build kept no base;
/// [`BuildResourceError::NotBuilt`] when no patch built it again;
/// [`BuildResourceError::SizeChanged`] when the rebuild made another size.
pub fn rebuild_in<T: ServedTable>(
    table: &T,
    key: T::Key,
    done: &mut Vec<ResourceKey>,
) -> Result<(), BuildResourceError> {
    if table.building(key) {
        return Err(BuildResourceError::Building);
    }
    done.push(table.keys(key).0);
    if rebuild_one(table, key)? != Some(0) {
        rebuild_readers(table, key, done);
    }
    Ok(())
}

/// Builds `key` again from its kept base, with the patches of every mod that
/// built the bytes the game holds, and writes it over them: how many bytes
/// changed, `None` before its first build.
///
/// # Errors
///
/// [`BuildResourceError::NoBase`] when its first build kept no base;
/// [`BuildResourceError::NotBuilt`] when no patch built it again;
/// [`BuildResourceError::SizeChanged`] when the rebuild made another size.
pub fn rebuild_one<T: ServedTable>(
    table: &T,
    key: T::Key,
) -> Result<Option<usize>, BuildResourceError> {
    let Some(held) = table.held(key) else {
        return Ok(None);
    };
    let kept = held.base.ok_or(BuildResourceError::NoBase)?;
    // SAFETY: `kept` lives past the build, so its base stays readable and
    // unchanged as `ServedTable` guarantees.
    let built = unsafe { build(table, key, table.base(&kept), Build::Game, Some(&held.by)) }?;
    if built.len != held.len {
        table.unbuilt(
            key,
            Unbuilt::SizeChanged {
                built: built.len,
                held: held.len,
            },
        );
        return Err(BuildResourceError::SizeChanged);
    }
    Ok(Some(table.write(key, kept, built)))
}

/// Rebuilds the keys whose builds read `key`, which changed, and in turn
/// those that read a key that changed.
///
/// Each key builds once, so keys that read each other end, and none of
/// `done`. A key whose build runs on this thread is left: its build reads
/// `key` as it is when it does.
pub fn rebuild_readers<T: ServedTable>(table: &T, key: T::Key, done: &mut Vec<ResourceKey>) {
    let mut next = table.readers(key);
    while let Some(reader) = next.pop() {
        let resource = table.keys(reader).0;
        if done.contains(&resource) || table.building(reader) {
            continue;
        }
        done.push(resource);
        if rebuild_one(table, reader).is_ok_and(|changed| changed != Some(0)) {
            next.extend(table.readers(reader));
        }
    }
}

/// Resource `key` as the game's first read would build it now, kept nowhere.
///
/// A served key builds from its base in the archive with the patches still
/// on, but its own re-keys when a re-key of `key` reads it
/// ([`Build::Unkeyed`]); any other key reads as the archive has the key
/// [`ServedTable::find`] names.
///
/// # Errors
///
/// [`BuildResourceError::Unsealed`] before the registrations seal;
/// [`BuildResourceError::Building`] for a key whose patches run on this
/// thread, which a patch reading it would build again without end;
/// [`BuildResourceError::NotInArchive`] for a key or base the archive lacks.
pub fn read_served<T: ServedTable>(
    table: &T,
    key: &ResourceKey,
) -> Result<Vec<u8>, BuildResourceError> {
    if !table.sealed() {
        return Err(BuildResourceError::Unsealed);
    }
    let served = match table.find(key) {
        Ok(served) => served,
        Err(read_as) => return archived(table, &read_as),
    };
    if table.building(served) {
        return Err(BuildResourceError::Building);
    }
    let base = archived(table, &table.keys(served).1)?;
    let kind = if REKEYING.get() == Some(*key) {
        Build::Unkeyed
    } else {
        Build::Scratch
    };
    read_built(table, served, base, kind)
}

/// `key` built for a mod's served read from `base`, its base's bytes, as
/// `kind`, [`Build::Scratch`] or [`Build::Unkeyed`], says.
///
/// When no patch built, it is `base` itself for a key patched in place and
/// the archive's bytes for a new one. The game's builds running on this
/// thread are noted as reading `key`.
///
/// # Errors
///
/// [`BuildResourceError::NotInArchive`] for a new key no patch built that the
/// archive lacks.
pub fn read_built<T: ServedTable>(
    table: &T,
    key: T::Key,
    base: Vec<u8>,
    kind: Build,
) -> Result<Vec<u8>, BuildResourceError> {
    table.note_read(key);
    let view = (base.as_ptr().expose_provenance(), base.len());
    // SAFETY: `base` is owned here and unchanged until the build ends.
    match unsafe { build(table, key, view, kind, None) } {
        Ok(built) => Ok(table.to_vec(built)),
        Err(BuildResourceError::NotBuilt) => {
            let (served, from) = table.keys(key);
            if served == from {
                Ok(base)
            } else {
                archived(table, &served)
            }
        }
        Err(why) => Err(why),
    }
}

/// The archive's resource `key`.
fn archived<T: ServedTable>(table: &T, key: &ResourceKey) -> Result<Vec<u8>, BuildResourceError> {
    table.archived(key).ok_or(BuildResourceError::NotInArchive)
}

#[cfg(test)]
mod tests;

//! The loader's side of the API: the `HostApi` table and what every entry
//! of it shares, from the calling mod's record to a panic's answer.

use std::sync::atomic::Ordering;

use enshwick_core::abi::{Level, ModHandle, Status};

use crate::engine::loader::{Loader, ModRecord, loader};
use crate::engine::mod_index::ModIndex;

mod api;
mod entries;
mod memory;

pub(crate) use api::API;

pub(crate) use memory::{find, slice};

/// The handle of the loader itself, which its own copy of the API uses to
/// log and read while it runs data packs.
pub(crate) const LOADER_HANDLE: ModHandle = ModHandle(0);

/// What an entry point answers when the loader's own code in it panicked.
trait Fallback {
    const FALLBACK: Self;
}

impl Fallback for Status {
    const FALLBACK: Self = Self::REFUSED;
}

impl Fallback for () {
    const FALLBACK: Self = ();
}

impl Fallback for Level {
    const FALLBACK: Self = Self::INFO;
}

impl Fallback for u8 {
    const FALLBACK: Self = 0;
}

impl Fallback for u32 {
    const FALLBACK: Self = 0;
}

impl Fallback for u64 {
    const FALLBACK: Self = 0;
}

/// Runs the body of the entry point `name`, so a panic in the loader's own
/// code never unwinds into the mod that called it: the call is refused and
/// the fault logged.
fn entry<R: Fallback>(name: &'static str, body: impl FnOnce() -> R) -> R {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
        .unwrap_or_else(|_| entry_fault(name))
}

#[cold]
fn entry_fault<R: Fallback>(name: &str) -> R {
    crate::log!(ERROR, "{name}: the loader panicked; the call is refused");
    R::FALLBACK
}

/// The calling mod of an entry point, with its index and record: refused
/// with [`Status::BAD_ARGS`] for a handle the loader never gave out and with
/// [`Status::FAULTED`] for a mod that is off, the one answer every entry
/// point gives a mod that is off.
fn calling(me: ModHandle) -> Result<(&'static Loader, ModIndex, &'static ModRecord), Status> {
    let loader = loader().ok_or(Status::BAD_ARGS)?;
    let (index, record) = loader.record(me).ok_or(Status::BAD_ARGS)?;
    if record.off.load(Ordering::Acquire) {
        return Err(Status::FAULTED);
    }
    Ok((loader, index, record))
}

/// [`calling`], for an entry point that needs only the loader; the
/// loader's own handle passes too.
fn caller(me: ModHandle) -> Result<&'static Loader, Status> {
    if me == LOADER_HANDLE {
        return loader().ok_or(Status::BAD_ARGS);
    }
    calling(me).map(|(loader, _, _)| loader)
}

/// [`calling`], for an entry point that needs the loader and the index.
fn caller_index(me: ModHandle) -> Result<(&'static Loader, ModIndex), Status> {
    calling(me).map(|(loader, index, _)| (loader, index))
}

/// The handle of the mod at `index`.
#[must_use]
pub(crate) fn handle(index: ModIndex) -> ModHandle {
    index.handle()
}

/// What the table answers a mod about itself and the build rows it reads.
impl Loader {
    /// The mod of handle `me`.
    fn record(&self, me: ModHandle) -> Option<(ModIndex, &ModRecord)> {
        let index = ModIndex::from_handle(me)?;
        Some((index, self.mods.get(index.get())?))
    }

    /// Whose build rows answer `me`'s lookups: a mod's, or for the loader's
    /// own copy of the API, which reads vanilla content for data packs, the
    /// loader's.
    fn rows_of(&self, me: ModHandle) -> Option<ModIndex> {
        ModIndex::looking_up(me, self.mods.len())
    }

    /// The mod's own build rows that match the running build.
    fn own_rows(
        &self,
        index: usize,
    ) -> impl Iterator<Item = &'static enshwick_core::abi::BuildRow> + '_ {
        let manifest = self.mods.get(index).map(|record| record.manifest);
        let rows: &'static [enshwick_core::abi::BuildRow] = match manifest {
            Some(m) if !m.builds.is_null() => {
                // SAFETY: `manifest::read` refused every manifest shorter
                // than `Manifest`, so `builds` and `build_count` were read
                // from the mod, and `manifest::own` copied the rows into
                // leaked loader memory: `build_count` rows, `'static`.
                unsafe { core::slice::from_raw_parts(m.builds, m.build_count) }
            }
            _ => &[],
        };
        rows.iter().filter(|row| {
            row.side == self.side
                && row.fingerprint.guid == self.fingerprint.guid()
                && row.fingerprint.age == self.fingerprint.age()
        })
    }

    fn resolve_symbol(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: a manifest's build row points at `symbol_count` symbols in
            // the mod's statics, and the mod's DLL stays loaded for the process.
            let symbols = unsafe { slice(row.symbols, row.symbol_count) };
            find(symbols, name)
        });
        own.or_else(|| self.build.symbol(name))
            .map(|rva| self.base + rva)
    }

    fn resolve_constant(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: a manifest's build row points at `constant_count`
            // constants in the mod's statics, and the mod's DLL stays loaded for
            // the process.
            let constants = unsafe { slice(row.constants, row.constant_count) };
            find(constants, name)
        });
        own.or_else(|| self.build.constant(name))
            .or_else(|| crate::engine::catalogue::constant(name, &self.reflection))
    }
}

#[cfg(test)]
mod headed_tests;

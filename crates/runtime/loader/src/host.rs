//! The loader's side of the API: the `HostApi` table, the layer every entry
//! of it passes through, and what the table answers a mod about itself.

use enshwick_core::abi::ModHandle;

use crate::engine::loader::{Loader, ModRecord};
use crate::engine::mod_index::ModIndex;

mod api;
mod entries;
pub(crate) mod layer;
mod memory;

pub(crate) use api::API;

pub(crate) use memory::{find, slice};

/// The handle of the loader itself, which its own copy of the API uses to
/// log and read while it runs data packs.
pub(crate) const LOADER_HANDLE: ModHandle = ModHandle(0);

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

//! The terrain ids a world's host serves, as the text its clients read before
//! they join, and the table a joining client takes from it.
//!
//! The host sends voxels as bytes (client 26430625, server 26430253), and a
//! client resolves its palette as a session starts, after the join check
//! (client 26430625), so a client that takes the host's table before it connects
//! names every byte the host's way.

use enshwick_core::content::Guid;

use super::{Changes, Material, Palette};
use crate::engine::mod_index::ModIndex;

/// Splits the entries of the text.
const ENTRIES: char = ',';
/// Splits the fields of one entry.
const FIELDS: char = ':';

/// One terrain id a host serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) id: u8,
    pub(crate) guid: Guid,
    /// The game's material its ground loads as without its mod.
    pub(crate) fallback: u8,
}

/// The text of the table `materials` serve: `<id>:<guid>:<fallback>` for
/// each, comma-separated.
pub(crate) fn text(materials: &[Material]) -> String {
    materials
        .iter()
        .map(|m| format!("{}{FIELDS}{}{FIELDS}{}", m.id, m.guid, m.fallback))
        .collect::<Vec<_>>()
        .join(&ENTRIES.to_string())
}

/// The table the host's `text` names, read as a peer's: `None` unless every
/// entry is a free id of `palette` named once, a GUID the game does not
/// name, and a fallback that is one of the game's terrain materials.
pub(crate) fn parse(text: &str, palette: &Palette) -> Option<Vec<Entry>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    let free = palette.free();
    let mut entries: Vec<Entry> = Vec::new();
    for entry in text.split(ENTRIES) {
        let mut fields = entry.split(FIELDS);
        let (Some(id), Some(guid), Some(fallback), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return None;
        };
        let entry = Entry {
            id: id.parse().ok()?,
            guid: Guid::try_parse(guid)?,
            fallback: fallback.parse().ok()?,
        };
        let fresh = entries
            .iter()
            .all(|e| e.id != entry.id && e.guid != entry.guid);
        if !fresh
            || !free.contains(&entry.id)
            || entry.guid == Guid::NIL
            || palette.id_of(entry.guid).is_some()
            || !palette.holds(entry.fallback)
        {
            return None;
        }
        entries.push(entry);
    }
    Some(entries)
}

/// A host's table as this side serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Adopted {
    /// The materials, at the host's ids.
    pub(crate) table: Vec<Material>,
    /// The first mod still on whose material the host serves at another id
    /// than the one the mod was told at init, or not at all: that mod reads
    /// a wrong id this session.
    pub(crate) moved: Option<ModIndex>,
}

/// The host's table `host` as this side serves it: a material of `own` that
/// a mod still `on` added keeps its rules and look at the host's id; every
/// other entry is a copy of the fallback the host names.
pub(crate) fn adopt(own: &[Material], host: &[Entry], on: impl Fn(ModIndex) -> bool) -> Adopted {
    let running: Vec<&Material> = own
        .iter()
        .filter(|m| m.owner != ModIndex::LOADER && on(m.owner))
        .collect();
    let moved = running
        .iter()
        .find(|m| !host.iter().any(|e| e.guid == m.guid && e.id == m.id))
        .map(|m| m.owner);
    let table = host
        .iter()
        .map(
            |entry| match running.iter().find(|m| m.guid == entry.guid) {
                Some(m) => Material {
                    id: entry.id,
                    ..**m
                },
                None => Material {
                    owner: ModIndex::LOADER,
                    guid: entry.guid,
                    id: entry.id,
                    like: entry.fallback,
                    fallback: entry.fallback,
                    changes: Changes::default(),
                },
            },
        )
        .collect();
    Adopted { table, moved }
}

#[cfg(test)]
mod tests;

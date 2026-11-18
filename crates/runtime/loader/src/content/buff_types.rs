//! Buff types of mods' own, served as one patch of the game's buff
//! collection with every mod's types after the vanilla ones.
//!
//! Each side reads the collection once and keeps it for the process
//! (client 26430625, server 26430253). A type's icon is a served UI image,
//! which a client's UI knows only once its registry listed the served images
//! again after arming: the patch names the icon only from then, and the
//! served path builds the collection again at that moment, as it does every
//! key whose build asked before, so the type shows its icon however early
//! the game built the collection.

use enshwick_core::__private::rules::buff_types::{Edit, Refusal, check};
use enshwick_core::abi::BuffTypeAdd;
use enshwick_core::buffs::BuffLayout;
use enshwick_core::content::Guid;

use crate::engine::mod_index::ModIndex;
use crate::engine::registry::Sealed;

/// Why a buff type was not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum BuffTypeError {
    /// Asked after init.
    #[error("asked after init")]
    TooLate,
    /// Its mod does not run on the clients that join, which look its id up
    /// too.
    #[error("a buff type needs a mod of side both, as clients look its id up too")]
    #[cfg_attr(
        all(not(windows), test),
        expect(dead_code, reason = "only the running game knows a mod's side")
    )]
    NotBoth,
    /// A change or flag the ABI does not name, or a negative life time.
    #[error("a change or flag it does not know, or a negative life time")]
    BadRequest,
    /// The game's collection does not take it: unread, no vanilla type to
    /// copy, or its id taken.
    #[error("{0}")]
    Refused(Refusal),
}

impl From<Sealed> for BuffTypeError {
    fn from(Sealed: Sealed) -> Self {
        Self::TooLate
    }
}

/// A mod's buff type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Added {
    owner: ModIndex,
    edit: Edit,
}

/// The edit `request` makes, checked against the vanilla collection
/// `vanilla` read with `layout` and the types `before` it of mods still on.
fn place(
    layout: &BuffLayout,
    vanilla: &[u8],
    before: &[Edit],
    request: &BuffTypeAdd,
) -> Result<Edit, BuffTypeError> {
    let edit = Edit::of(request).ok_or(BuffTypeError::BadRequest)?;
    check(layout, vanilla, &edit, before).map_err(BuffTypeError::Refused)?;
    Ok(edit)
}

/// The edits of `added` served this run: those of the mods `on` names, in
/// load order.
fn serving(added: &[Added], on: impl Fn(ModIndex) -> bool) -> Vec<Edit> {
    added
        .iter()
        .filter(|added| on(added.owner))
        .map(|added| added.edit)
        .collect()
}

/// `types` as a build names them: each with its icon only when `shows` it,
/// since naming a UI image the client's UI does not know ends the game.
fn named(types: &[Edit], shows: impl Fn(Guid) -> bool) -> Vec<Edit> {
    types
        .iter()
        .map(|edit| match edit.icon() {
            Some(icon) if !shows(icon) => edit.without_icon(),
            _ => *edit,
        })
        .collect()
}

/// The feature the buff patch's switch names.
#[cfg(windows)]
pub(crate) const FEATURE: &str = "buff types";

#[cfg(windows)]
pub(crate) use running::{add, admits, sealed, serve};

/// The buff types in the running game: mods' registrations, the archive's
/// collection and the loader's patch.
#[cfg(windows)]
mod running {
    use std::sync::OnceLock;

    use enshwick_core::__private::rules::buff_types::{Refusal, build};
    use enshwick_core::abi::{BuffTypeAdd, BuffTypeId, ResourceKey};
    use enshwick_core::buffs::{BUFF_COLLECTION, BUFF_COLLECTION_RESOURCE, BuffLayout};

    use super::{Added, BuffTypeError, named, place, serving};
    use crate::content::served::{Builder, mod_on};
    use crate::engine::catalogue::Reflected;
    use crate::engine::loader::Loader;
    use crate::engine::mod_index::ModIndex;
    use crate::engine::registry::Registry;

    /// The buff types of the run.
    struct Buffs {
        /// The types mods added, in load order.
        types: Registry<Added>,
        /// The build's layout, the collection's key and its vanilla bytes,
        /// read once from the archive; `None` when the archive or
        /// reflection lacks them.
        collection: OnceLock<Option<(BuffLayout, ResourceKey, Vec<u8>)>>,
    }

    static BUFFS: Buffs = Buffs {
        types: Registry::new(),
        collection: OnceLock::new(),
    };

    /// The build's layout, the collection's key and its vanilla bytes.
    fn collection(loader: &Loader) -> Option<&'static (BuffLayout, ResourceKey, Vec<u8>)> {
        BUFFS
            .collection
            .get_or_init(|| {
                let layout = BuffLayout::resolve(&Reflected(&loader.reflection)).ok()?;
                let hash = enshwick_gamedata::catalogue::type_hash(BUFF_COLLECTION_RESOURCE.id())?;
                let key = BUFF_COLLECTION.key_by_hash(hash);
                let vanilla = crate::engine::archive::game()?.resource(&key)?;
                Some((layout, key, vanilla))
            })
            .as_ref()
    }

    /// Adds the buff type `request` of mod `owner` asks for and returns its
    /// game id; `owner` pairs the mod with whether it runs on the clients
    /// that join.
    ///
    /// # Errors
    ///
    /// [`BuffTypeError`] after init, for a mod clients do not run, without
    /// the collection, for a request it does not know, a `like` the game
    /// lacks and an id the game or a mod before has.
    pub(crate) fn add(
        loader: &Loader,
        (owner, joins): (ModIndex, bool),
        request: &BuffTypeAdd,
    ) -> Result<BuffTypeId, BuffTypeError> {
        admits(joins)?;
        let unread = BuffTypeError::Refused(Refusal::Unread);
        let (layout, _, vanilla) = collection(loader).ok_or(unread)?;
        BUFFS.types.edit(|added| {
            let edit = place(layout, vanilla, &serving(added, mod_on), request)?;
            added.push(Added { owner, edit });
            Ok(edit.id())
        })?
    }

    /// Whether a mod may add a buff type now: during init, and only a mod
    /// that `joins` clients, which look its id up too. The host entry asks
    /// it before the other refusals, so a late call reads as late.
    ///
    /// # Errors
    ///
    /// [`BuffTypeError::TooLate`] after init, [`BuffTypeError::NotBoth`] for a
    /// mod clients do not run.
    pub(crate) fn admits(joins: bool) -> Result<(), BuffTypeError> {
        if BUFFS.types.is_sealed() {
            return Err(BuffTypeError::TooLate);
        }
        if !joins {
            return Err(BuffTypeError::NotBoth);
        }
        Ok(())
    }

    /// Seals the mods' types: none is added after.
    pub(crate) fn sealed() -> &'static [Added] {
        BUFFS.types.seal()
    }

    /// The loader's patch of the collection with the types `added` of the
    /// mods on at each build; `None` when there are none to serve.
    pub(crate) fn serve(
        loader: &'static Loader,
        added: &'static [Added],
    ) -> Option<(ResourceKey, Builder)> {
        let types = serving(added, mod_on);
        if types.is_empty() {
            return None;
        }
        // `add` took every type with the collection read.
        let &(layout, key, _) = collection(loader)?;
        for edit in &types {
            crate::log!(INFO, "buff type {:?} served", edit.id());
        }
        let builder: Builder = Box::new(move |base: Vec<u8>| {
            let types = named(&serving(added, mod_on), crate::content::textures::knows);
            let mut out = Vec::new();
            let Some(left) = build(&layout, &base, &types, &mut out) else {
                crate::log!(
                    WARN,
                    "buff types left out: the game's buff collection does not read with this build's layout"
                );
                return None;
            };
            if !left.is_empty() {
                crate::log!(
                    WARN,
                    "buff types {left:?} left out: a patch before took their ids or vanilla types"
                );
            }
            Some(out)
        });
        Some((key, builder))
    }
}

#[cfg(test)]
mod tests;

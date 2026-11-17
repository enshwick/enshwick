//! The content renames mods declare: content saved under an old GUID reads
//! as the content now under the new one.

use std::sync::OnceLock;

use enshwick_core::abi::ContentRename;
use rustc_hash::FxHashMap;

#[cfg(windows)]
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::registry::Registry;
use enshwick_core::content::Guid;

/// One mod's renames, old GUID to new.
#[derive(Debug)]
struct Declared {
    owner: ModIndex,
    renames: Vec<(Guid, Guid)>,
}

/// Open while mods initialise, sealed by [`renamed`].
static DECLARED: Registry<Declared> = Registry::new();
/// Each old GUID to the end of its chain, once sealed.
static RENAMES: OnceLock<FxHashMap<Guid, Guid>> = OnceLock::new();

/// Why a mod's renames were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RenameError {
    /// A rename of an id to itself.
    #[error("a rename to itself")]
    ToItself,
    /// Renames that lead back to where they start.
    #[error("renames in a cycle")]
    Cycle,
    /// The mod's second declaration.
    #[error("the mod declared its renames already")]
    Twice,
    /// Content is sealed.
    #[error("asked after content was sealed")]
    Sealed,
}

/// Keeps the renames of the mod `owner`; [`RenameError`] for a rename to
/// itself, renames in a cycle, a mod's second declaration, or one after
/// content is sealed.
pub(crate) fn declare(owner: ModIndex, renames: &[ContentRename]) -> Result<(), RenameError> {
    add(&DECLARED, owner, renames)
}

fn add(
    declared: &Registry<Declared>,
    owner: ModIndex,
    renames: &[ContentRename],
) -> Result<(), RenameError> {
    if renames.iter().any(|r| r.from == r.to) {
        return Err(RenameError::ToItself);
    }
    let direct: FxHashMap<Guid, Guid> = renames.iter().map(|r| (r.from, r.to)).collect();
    // A chain's end is no rename's old GUID; a cycle has no end.
    if follow(&direct).values().any(|end| direct.contains_key(end)) {
        return Err(RenameError::Cycle);
    }
    let renames = renames.iter().map(|r| (r.from, r.to)).collect();
    declared
        .edit(|list| {
            if list.iter().any(|d| d.owner == owner) {
                return Err(RenameError::Twice);
            }
            list.push(Declared { owner, renames });
            Ok(())
        })
        .unwrap_or(Err(RenameError::Sealed))
}

/// Gives `guid`, which mod `owner` of `loader` has just declared, back to
/// it whatever the load order: every other mod's renames from or to it are
/// dropped, with the resources those mods serve under it through them, and
/// a warning names both mods.
#[cfg(windows)]
pub(crate) fn claim(loader: &Loader, owner: ModIndex, guid: Guid) {
    let losers = take(&DECLARED, owner, guid);
    if losers.is_empty() {
        return;
    }
    crate::content::served::unserve(&losers, guid);
    let id = |index: ModIndex| loader.mods.get(index.get()).map_or("", |m| &*m.id);
    for &loser in &losers {
        crate::log!(
            WARN,
            "{} declared content {guid}, so the renames of {} from or to it and what it serves under it are dropped",
            id(owner),
            id(loser)
        );
    }
}

/// Takes every rename from or to `guid` out of the renames of mods other
/// than `owner`; the mods that lost one.
fn take(declared: &Registry<Declared>, owner: ModIndex, guid: Guid) -> Vec<ModIndex> {
    declared
        .edit(|list| {
            list.iter_mut()
                .filter(|d| d.owner != owner)
                .filter_map(|d| {
                    let before = d.renames.len();
                    d.renames.retain(|&(from, to)| from != guid && to != guid);
                    (d.renames.len() < before).then_some(d.owner)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether mod `owner` renamed content from `guid`: it serves its renamed
/// content under the old GUID too.
pub(crate) fn renames_from(owner: ModIndex, guid: Guid) -> bool {
    DECLARED.any(|d| d.owner == owner && d.renames.iter().any(|&(from, _)| from == guid))
}

/// The renames of every mod `on` keeps, old GUID to the end of its chain:
/// sealed on the first call, after every mod is initialised.
pub(crate) fn renamed(on: impl Fn(ModIndex) -> bool) -> &'static FxHashMap<Guid, Guid> {
    RENAMES.get_or_init(|| sealed(&DECLARED, on))
}

fn sealed(declared: &Registry<Declared>, on: impl Fn(ModIndex) -> bool) -> FxHashMap<Guid, Guid> {
    let direct: FxHashMap<Guid, Guid> = declared
        .seal()
        .iter()
        .filter(|d| on(d.owner))
        .flat_map(|d| d.renames.iter().copied())
        .collect();
    follow(&direct)
}

/// Each old GUID to the end of its chain of renames, A to C for A to B and
/// B to C; a cycle stops where it would come back.
fn follow(direct: &FxHashMap<Guid, Guid>) -> FxHashMap<Guid, Guid> {
    direct
        .iter()
        .map(|(&from, &first)| {
            let mut to = first;
            for _ in 0..direct.len() {
                match direct.get(&to) {
                    Some(&next) if next != from => to = next,
                    _ => break,
                }
            }
            (from, to)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(from: u8, to: u8) -> ContentRename {
        ContentRename {
            from: Guid([from; 16]),
            to: Guid([to; 16]),
        }
    }

    #[test]
    fn content_renamed_twice_reads_as_the_last_guid() {
        let declared = Registry::new();
        let (first, second) = (ModIndex::new(0), ModIndex::new(1));
        assert_eq!(add(&declared, first, &[rename(1, 2)]), Ok(()));
        assert_eq!(add(&declared, second, &[rename(2, 3)]), Ok(()));
        let renames = sealed(&declared, |_| true);
        assert_eq!(renames[&Guid([1; 16])], Guid([3; 16]));
        assert_eq!(renames[&Guid([2; 16])], Guid([3; 16]));
        assert!(!renames.contains_key(&Guid([3; 16])));
    }

    #[test]
    fn a_mod_that_is_off_renames_nothing() {
        let declared = Registry::new();
        let (on, off) = (ModIndex::new(0), ModIndex::new(1));
        assert_eq!(add(&declared, on, &[rename(1, 2)]), Ok(()));
        assert_eq!(add(&declared, off, &[rename(4, 5)]), Ok(()));
        let renames = sealed(&declared, |owner| owner == on);
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[&Guid([1; 16])], Guid([2; 16]));
    }

    #[test]
    fn a_rename_to_itself_a_second_declaration_or_a_late_one_is_refused() {
        let declared = Registry::new();
        let owner = ModIndex::new(0);
        assert_eq!(
            add(&declared, owner, &[rename(1, 1)]),
            Err(RenameError::ToItself)
        );
        assert_eq!(add(&declared, owner, &[rename(1, 2)]), Ok(()));
        assert_eq!(
            add(&declared, owner, &[rename(3, 4)]),
            Err(RenameError::Twice)
        );
        let _sealed = sealed(&declared, |_| true);
        assert_eq!(
            add(&declared, ModIndex::new(1), &[rename(5, 6)]),
            Err(RenameError::Sealed)
        );
    }

    #[test]
    fn one_mods_renames_in_a_cycle_are_refused_and_a_chain_is_kept() {
        let declared = Registry::new();
        assert_eq!(
            add(&declared, ModIndex::new(0), &[rename(1, 2), rename(2, 1)]),
            Err(RenameError::Cycle)
        );
        assert_eq!(
            add(
                &declared,
                ModIndex::new(1),
                &[rename(3, 4), rename(4, 5), rename(5, 4)]
            ),
            Err(RenameError::Cycle)
        );
        assert_eq!(
            add(&declared, ModIndex::new(2), &[rename(7, 8), rename(6, 7)]),
            Ok(())
        );
    }

    #[test]
    fn a_mod_declaring_a_guid_takes_it_out_of_other_mods_renames_but_its_own() {
        let declared = Registry::new();
        let (earlier, owner) = (ModIndex::new(0), ModIndex::new(1));
        assert_eq!(
            add(
                &declared,
                earlier,
                &[rename(1, 9), rename(2, 1), rename(3, 4)]
            ),
            Ok(())
        );
        assert_eq!(add(&declared, owner, &[rename(5, 1)]), Ok(()));
        assert_eq!(take(&declared, owner, Guid([1; 16])), [earlier]);
        assert_eq!(take(&declared, owner, Guid([1; 16])), []);
        let renames = sealed(&declared, |_| true);
        assert_eq!(renames.len(), 2);
        assert_eq!(renames[&Guid([3; 16])], Guid([4; 16]));
        assert_eq!(renames[&Guid([5; 16])], Guid([1; 16]));
        assert_eq!(take(&declared, owner, Guid([3; 16])), [], "sealed");
    }

    #[test]
    fn a_cycle_of_renames_stops_before_it_comes_back() {
        let direct = [
            (Guid([1; 16]), Guid([2; 16])),
            (Guid([2; 16]), Guid([1; 16])),
        ]
        .into_iter()
        .collect();
        let followed = follow(&direct);
        assert_eq!(followed[&Guid([1; 16])], Guid([2; 16]));
        assert_eq!(followed[&Guid([2; 16])], Guid([1; 16]));
    }
}

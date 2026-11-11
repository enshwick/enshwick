//! The content renames mods declare: content saved under an old GUID reads
//! as the content now under the new one.

use std::sync::OnceLock;

use enshwick_core::abi::{ContentRename, Status};
use rustc_hash::FxHashMap;

use crate::mod_index::ModIndex;
use crate::registry::Registry;

/// One mod's renames, old GUID to new.
#[derive(Debug)]
struct Declared {
    owner: ModIndex,
    renames: Vec<([u8; 16], [u8; 16])>,
}

/// Open while mods initialise, sealed by [`renamed`].
static DECLARED: Registry<Declared> = Registry::new();
/// Each old GUID to the end of its chain, once sealed.
static RENAMES: OnceLock<FxHashMap<[u8; 16], [u8; 16]>> = OnceLock::new();

/// Keeps the renames of the mod `owner`; [`Status::REFUSED`] for a rename to
/// itself, a mod's second declaration, or one after content is sealed.
pub(crate) fn declare(owner: ModIndex, renames: &[ContentRename]) -> Status {
    add(&DECLARED, owner, renames)
}

fn add(declared: &Registry<Declared>, owner: ModIndex, renames: &[ContentRename]) -> Status {
    if renames.iter().any(|r| r.from == r.to) {
        return Status::REFUSED;
    }
    let renames = renames.iter().map(|r| (r.from, r.to)).collect();
    declared
        .edit(|list| {
            if list.iter().any(|d| d.owner == owner) {
                return Status::REFUSED;
            }
            list.push(Declared { owner, renames });
            Status::OK
        })
        .unwrap_or(Status::REFUSED)
}

/// The renames of every mod `on` keeps, old GUID to the end of its chain:
/// sealed on the first call, after every mod is initialised.
pub(crate) fn renamed(on: impl Fn(ModIndex) -> bool) -> &'static FxHashMap<[u8; 16], [u8; 16]> {
    RENAMES.get_or_init(|| sealed(&DECLARED, on))
}

fn sealed(
    declared: &Registry<Declared>,
    on: impl Fn(ModIndex) -> bool,
) -> FxHashMap<[u8; 16], [u8; 16]> {
    let direct: FxHashMap<[u8; 16], [u8; 16]> = declared
        .seal()
        .iter()
        .filter(|d| on(d.owner))
        .flat_map(|d| d.renames.iter().copied())
        .collect();
    follow(&direct)
}

/// Each old GUID to the end of its chain of renames, A to C for A to B and
/// B to C; a cycle stops where it would come back.
fn follow(direct: &FxHashMap<[u8; 16], [u8; 16]>) -> FxHashMap<[u8; 16], [u8; 16]> {
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
            from: [from; 16],
            to: [to; 16],
        }
    }

    #[test]
    fn content_renamed_twice_reads_as_the_last_guid() {
        let declared = Registry::new();
        let (first, second) = (ModIndex::new(0), ModIndex::new(1));
        assert_eq!(add(&declared, first, &[rename(1, 2)]), Status::OK);
        assert_eq!(add(&declared, second, &[rename(2, 3)]), Status::OK);
        let renames = sealed(&declared, |_| true);
        assert_eq!(renames[&[1; 16]], [3; 16]);
        assert_eq!(renames[&[2; 16]], [3; 16]);
        assert!(!renames.contains_key(&[3; 16]));
    }

    #[test]
    fn a_mod_that_is_off_renames_nothing() {
        let declared = Registry::new();
        let (on, off) = (ModIndex::new(0), ModIndex::new(1));
        assert_eq!(add(&declared, on, &[rename(1, 2)]), Status::OK);
        assert_eq!(add(&declared, off, &[rename(4, 5)]), Status::OK);
        let renames = sealed(&declared, |owner| owner == on);
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[&[1; 16]], [2; 16]);
    }

    #[test]
    fn a_rename_to_itself_a_second_declaration_or_a_late_one_is_refused() {
        let declared = Registry::new();
        let owner = ModIndex::new(0);
        assert_eq!(add(&declared, owner, &[rename(1, 1)]), Status::REFUSED);
        assert_eq!(add(&declared, owner, &[rename(1, 2)]), Status::OK);
        assert_eq!(add(&declared, owner, &[rename(3, 4)]), Status::REFUSED);
        let _sealed = sealed(&declared, |_| true);
        assert_eq!(
            add(&declared, ModIndex::new(1), &[rename(5, 6)]),
            Status::REFUSED
        );
    }

    #[test]
    fn a_cycle_of_renames_stops_before_it_comes_back() {
        let direct = [([1; 16], [2; 16]), ([2; 16], [1; 16])]
            .into_iter()
            .collect();
        let followed = follow(&direct);
        assert_eq!(followed[&[1; 16]], [2; 16]);
        assert_eq!(followed[&[2; 16]], [1; 16]);
    }
}

//! The rule the loader and the fake host share when a mod declares a GUID
//! that another mod's content renames name.

use crate::content::Guid;

/// Takes `guid`, which another mod has just declared, out of one mod's
/// `renames`; whether anything changed.
///
/// A rename from it goes, and a rename onto it ends where the same mod's
/// rename from it ended, so the rest of that chain still reads.
///
/// # Examples
///
/// ```
/// use enshwick_base::__internal::renames::release;
/// use enshwick_base::content::Guid;
///
/// let [y, x, g] = [1, 2, 3].map(|n| Guid([n; 16]));
/// let mut renames = vec![(y, x), (x, g)];
/// assert!(release(&mut renames, x));
/// assert_eq!(renames, [(y, g)]);
/// ```
pub fn release(renames: &mut Vec<(Guid, Guid)>, guid: Guid) -> bool {
    let onward = renames
        .iter()
        .find(|&&(from, _)| from == guid)
        .map(|&(_, to)| to);
    let mut changed = false;
    renames.retain_mut(|(from, to)| {
        if *from != guid && *to != guid {
            return true;
        }
        changed = true;
        match onward {
            Some(end) if *from != guid => {
                *to = end;
                true
            }
            _ => false,
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guid(n: u8) -> Guid {
        Guid([n; 16])
    }

    #[test]
    fn a_rename_from_the_guid_goes_and_a_chain_through_it_skips_it() {
        let mut renames = vec![(guid(1), guid(2)), (guid(2), guid(3)), (guid(4), guid(5))];
        assert!(release(&mut renames, guid(2)));
        assert_eq!(renames, [(guid(1), guid(3)), (guid(4), guid(5))]);
    }

    #[test]
    fn a_rename_onto_the_guid_with_nothing_onward_goes() {
        let mut renames = vec![(guid(1), guid(2)), (guid(4), guid(5))];
        assert!(release(&mut renames, guid(2)));
        assert_eq!(renames, [(guid(4), guid(5))]);
    }

    #[test]
    fn renames_that_do_not_name_the_guid_stay_unchanged() {
        let mut renames = vec![(guid(1), guid(2))];
        assert!(!release(&mut renames, guid(9)));
        assert_eq!(renames, [(guid(1), guid(2))]);
    }
}

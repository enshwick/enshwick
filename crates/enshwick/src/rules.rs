//! The rules the loader and the fake host both refuse a mod's requests by.

use crate::abi::{HookFlags, RateId};
use crate::rates::Rate;

/// Whether a hook names both a target and a detour: without either it is a
/// bad request.
#[must_use]
pub fn is_complete_hook(target: u64, detour: usize) -> bool {
    target != 0 && detour != 0
}

/// Whether a new hook clashes with one a target holds: one mod links a
/// target once, and an exclusive link shares it with nobody.
#[must_use]
pub fn hooks_clash(same_mod: bool, held: HookFlags, new: HookFlags) -> bool {
    same_mod || held.contains(HookFlags::EXCLUSIVE) || new.contains(HookFlags::EXCLUSIVE)
}

/// Whether a code patch over `span` (address and length) patches at least
/// one byte and lies inside `text`, the game's code section.
#[must_use]
pub fn patch_fits((address, len): (usize, usize), text: (usize, usize)) -> bool {
    len != 0 && address >= text.0 && address.saturating_add(len) <= text.0.saturating_add(text.1)
}

/// Whether two spans of bytes, each an address and a length, share a byte.
#[must_use]
pub fn spans_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.0.saturating_add(b.1) && b.0 < a.0.saturating_add(a.1)
}

/// Whether a point's name is `<id>:<point>` under one of the defining mod's
/// `ids`: its id and the ids it had before a rename.
#[must_use]
pub fn is_named_after(name: &str, ids: &[&str]) -> bool {
    name.split_once(':')
        .is_some_and(|(id, what)| ids.contains(&id) && !what.is_empty())
}

/// Whether a mod may declare the rate `rate`: one clients count too needs a
/// mod that `joins`, so both sides scale it alike.
#[must_use]
pub fn may_declare_rate(rate: RateId, joins: bool) -> bool {
    joins || !Rate::from_id(rate).is_some_and(Rate::seen_by_clients)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_needs_a_target_and_a_detour() {
        assert!(is_complete_hook(1, 1));
        assert!(!is_complete_hook(0, 1));
        assert!(!is_complete_hook(1, 0));
    }

    #[test]
    fn a_target_takes_one_link_per_mod_and_none_beside_an_exclusive_one() {
        let none = HookFlags::NONE;
        assert!(!hooks_clash(false, none, none));
        assert!(hooks_clash(true, none, none));
        assert!(hooks_clash(false, HookFlags::EXCLUSIVE, none));
        assert!(hooks_clash(false, none, HookFlags::EXCLUSIVE));
    }

    #[test]
    fn a_patch_fits_only_inside_the_code_and_with_bytes() {
        let text = (100, 10);
        assert!(patch_fits((100, 10), text));
        assert!(patch_fits((104, 2), text));
        assert!(!patch_fits((104, 0), text));
        assert!(!patch_fits((99, 2), text));
        assert!(!patch_fits((105, 6), text));
        assert!(!patch_fits((usize::MAX, 2), text));
    }

    #[test]
    fn spans_overlap_only_when_they_share_a_byte() {
        assert!(spans_overlap((0, 4), (2, 4)));
        assert!(spans_overlap((2, 4), (0, 4)));
        assert!(!spans_overlap((0, 4), (4, 4)));
        assert!(!spans_overlap((0, 0), (0, 4)));
    }

    #[test]
    fn a_name_is_under_one_of_its_mods_ids() {
        let ids = ["you.mod", "you.old"];
        assert!(is_named_after("you.mod:limit", &ids));
        assert!(is_named_after("you.old:limit", &ids));
        assert!(!is_named_after("you.mod:", &ids));
        assert!(!is_named_after("other.mod:limit", &ids));
        assert!(!is_named_after("limit", &ids));
    }

    #[test]
    fn a_rate_clients_count_needs_a_mod_that_joins() {
        let seen = Rate::SkillPoints.id();
        assert!(seen_by_clients(seen));
        assert!(!may_declare_rate(seen, false));
        assert!(may_declare_rate(seen, true));
        let own = Rate::all()
            .find(|rate| !rate.seen_by_clients())
            .map(Rate::id);
        assert!(own.is_some_and(|own| may_declare_rate(own, false)));
    }

    fn seen_by_clients(rate: RateId) -> bool {
        Rate::from_id(rate).is_some_and(Rate::seen_by_clients)
    }
}

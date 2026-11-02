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

/// Whether a point's name is `<id>:<point>` under the defining mod's `id`.
#[must_use]
pub fn is_named_after(name: &str, id: &str) -> bool {
    name.split_once(':')
        .is_some_and(|(owner, what)| owner == id && !what.is_empty())
}

/// Whether a mod may declare the rate `rate`: one clients count too needs a
/// mod that `joins`, so both sides scale it alike.
#[must_use]
pub fn may_declare_rate(rate: RateId, joins: bool) -> bool {
    joins || !Rate::from_id(rate).is_some_and(Rate::seen_by_clients)
}

/// The first of `exports` that is not `<kind>:<local name>`, the kind one
/// of [`Kind::ALL`](crate::schema_section::Kind::ALL) and the local name 1
/// to 128 of `[a-z0-9_./-]`, with no mod id; `None` when every one is.
///
/// The loader refuses a manifest with such an export, and the fake does too.
#[must_use]
pub fn bad_export<S: AsRef<str>>(exports: &[S]) -> Option<&str> {
    let local_ok = |local: &str| {
        (1..=128).contains(&local.len())
            && local.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'_' | b'.' | b'/' | b'-')
            })
    };
    exports.iter().map(AsRef::as_ref).find(|export| {
        crate::schema_section::Kind::of_export(export).is_none_or(|(_, local)| !local_ok(local))
    })
}

/// Why a mod may not use the `kind` `name`, `<owner>:<what>`, whose owner
/// exports `exports`: the loader's and the fake's one message, naming the
/// missing export with its kind.
#[must_use]
pub fn unexported_reason<S: AsRef<str>>(
    kind: crate::schema_section::Kind,
    name: &str,
    exports: &[S],
) -> String {
    let (owner, what) = name.split_once(':').unwrap_or((name, ""));
    let offered = if exports.is_empty() {
        "it exports nothing".to_owned()
    } else {
        let names: Vec<&str> = exports.iter().map(AsRef::as_ref).collect();
        format!("it exports {}", names.join(", "))
    };
    format!(
        "{} {name}: {owner} does not export {}:{what}; {offered}",
        kind.label(),
        kind.word()
    )
}

/// Whether a mod's `exports`, each `<kind>:<local name>`, let other mods use
/// its `kind` `name`.
///
/// `name` is a local name, exported when `kind:name` is, or, for `content`
/// and `stat`, the name it is a part of (`content:locker` for
/// `locker.recipe`).
#[must_use]
pub fn exports_name<S: AsRef<str>>(
    exports: &[S],
    kind: crate::schema_section::Kind,
    name: &str,
) -> bool {
    let exported = |local: &str| {
        exports
            .iter()
            .any(|e| crate::schema_section::Kind::of_export(e.as_ref()) == Some((kind, local)))
    };
    exported(name)
        || (matches!(
            kind,
            crate::schema_section::Kind::Content | crate::schema_section::Kind::Stat
        ) && name.match_indices('.').any(|(at, _)| exported(&name[..at])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_section::Kind;

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
    fn a_name_is_under_its_mod_s_id() {
        assert!(is_named_after("you.mod:limit", "you.mod"));
        assert!(!is_named_after("you.mod:", "you.mod"));
        assert!(!is_named_after("other.mod:limit", "you.mod"));
        assert!(!is_named_after("limit", "you.mod"));
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

    #[test]
    fn an_exported_content_name_exports_its_parts_too() {
        let exports = ["content:locker", "content:bench.recipe"];
        let content = |name| exports_name(&exports, Kind::Content, name);
        assert!(content("locker"));
        assert!(content("locker.recipe"));
        assert!(content("locker.icon.png"));
        assert!(content("bench.recipe"));
        assert!(!content("bench"));
        assert!(!content("bench.name"));
        assert!(!content("lockers"));
        assert!(!content("locker-key"));
        assert!(!exports_name::<&str>(&[], Kind::Content, "locker"));
    }

    #[test]
    fn an_export_without_a_known_kind_or_a_local_name_is_bad() {
        assert_eq!(bad_export(&["point:price", "content:a.b/c-d_e"]), None);
        for bad in [
            "price",
            "greeting:hi",
            "point:",
            "point:you.a:price",
            "content:Big",
        ] {
            assert_eq!(bad_export(&["event:sold", bad]), Some(bad), "{bad}");
        }
        let long = format!("content:{}", "a".repeat(129));
        assert_eq!(bad_export(&[long.as_str()]), Some(long.as_str()));
    }

    #[test]
    fn an_export_opens_only_its_own_kind() {
        let exports = ["point:price", "event:sold"];
        assert!(exports_name(&exports, Kind::Point, "price"));
        assert!(!exports_name(&exports, Kind::Content, "price"));
        assert!(!exports_name(&exports, Kind::Stat, "price"));
        assert!(!exports_name(&exports, Kind::Event, "price"));
        assert!(!exports_name(&exports, Kind::Call, "sold"));
        assert!(!exports_name(&exports, Kind::Point, "price.part"));
    }

    fn seen_by_clients(rate: RateId) -> bool {
        Rate::from_id(rate).is_some_and(Rate::seen_by_clients)
    }
}

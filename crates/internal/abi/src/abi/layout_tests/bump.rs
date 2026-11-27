//! The version bump rule against pairs of layouts, each a change a layout
//! can see.

use super::*;

/// Pairs of structs, each a change a layout can see.
mod fixture {
    use super::Header;

    #[repr(C)]
    pub(super) struct Before {
        pub(super) call: extern "system" fn(u32) -> u32,
    }

    #[repr(C)]
    pub(super) struct After {
        pub(super) call: extern "system" fn(u64, u32) -> u32,
    }

    #[repr(C)]
    pub(super) struct Headed {
        pub(super) header: Header,
        pub(super) value: u32,
    }

    #[repr(C)]
    pub(super) struct HeadedGrown {
        pub(super) header: Header,
        pub(super) value: u32,
        pub(super) added: u32,
    }

    #[repr(C)]
    pub(super) struct Padded {
        pub(super) header: Header,
        pub(super) wide: u64,
        pub(super) short: u32,
    }

    #[repr(C)]
    pub(super) struct PaddedGrown {
        pub(super) header: Header,
        pub(super) wide: u64,
        pub(super) short: u32,
        pub(super) added: u32,
        pub(super) more: u64,
    }

    #[repr(C)]
    pub(super) struct Plain {
        pub(super) value: u32,
    }

    #[repr(C)]
    pub(super) struct PlainGrown {
        pub(super) value: u32,
        pub(super) added: u32,
    }
}

use fixture::{After, Before, Headed, HeadedGrown, Padded, PaddedGrown, Plain, PlainGrown};

/// A layout of the named shapes.
fn layout(types: &[(&str, Shape)]) -> Snapshot {
    Snapshot {
        types: types
            .iter()
            .map(|(name, shape)| ((*name).to_owned(), shape.clone()))
            .collect(),
        born: BTreeMap::new(),
        constants: BTreeMap::new(),
    }
}

/// `layout` with `Fixture`'s [`Headed::BORN`](crate::abi::Headed::BORN) at `born`.
fn born_at(mut layout: Snapshot, born: usize) -> Snapshot {
    layout.born.insert("Fixture".to_owned(), born);
    layout
}

/// Whether `now` fits the change of one type `Fixture` from `old` at ABI
/// 5.2 to `new`.
fn fits(old: Shape, new: Shape, now: Version) -> bool {
    let old = layout(&[("Fixture", old)]);
    let new = layout(&[("Fixture", new)]);
    wrong_bump((5, 2), &old, &new, now).is_none()
}

#[test]
fn an_unchanged_layout_keeps_its_version() {
    let headed = || shape!(Headed { header, value });
    assert!(fits(headed(), headed(), (5, 2)));
    assert!(!fits(headed(), headed(), (5, 3)));
    assert!(fits(headed(), headed(), (6, 0)));
}

#[test]
fn a_field_appended_to_a_headed_struct_needs_the_next_minor() {
    let old = || shape!(Headed { header, value });
    let new = || {
        shape!(HeadedGrown {
            header,
            value,
            added
        })
    };
    assert!(fits(old(), new(), (5, 3)));
    assert!(!fits(old(), new(), (5, 2)), "a missing bump fails");
    assert!(!fits(old(), new(), (5, 4)), "a skipped minor fails");
    assert!(fits(old(), new(), (6, 0)));
}

#[test]
fn a_field_appended_into_the_old_tail_padding_needs_a_new_major() {
    let old = || {
        shape!(Padded {
            header,
            wide,
            short
        })
    };
    let new = || {
        shape!(PaddedGrown {
            header,
            wide,
            short,
            added,
            more
        })
    };
    assert!(new().fields[3].offset < old().size);
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
}

#[test]
fn the_major_moves_only_to_the_next_one() {
    let headed = || shape!(Headed { header, value });
    assert!(!fits(headed(), headed(), (4, 0)));
    assert!(!fits(headed(), headed(), (1, 0)));
    assert!(!fits(headed(), headed(), (7, 0)), "a skipped major fails");
}

#[test]
fn the_last_alpha_major_steps_back_once_to_the_stable_one() {
    let old = born_at(layout(&[("Fixture", shape!(Headed { header, value }))]), 12);
    let grown = layout(&[(
        "Fixture",
        shape!(HeadedGrown {
            header,
            value,
            added
        }),
    )]);
    let last = *ALPHA_MAJORS.end();
    assert_eq!(wrong_bump((last, 29), &old, &grown, (1, 0)), None);
    assert!(wrong_bump((last, 29), &old, &grown, (1, 1)).is_some());
    assert!(
        wrong_bump((last, 29), &old, &born_at(grown.clone(), 12), (1, 0)).is_some(),
        "the stable major starts again at the size"
    );
    assert!(wrong_bump((2, 0), &old, &grown, (1, 0)).is_some());
    assert_eq!(wrong_bump((1, 0), &old, &grown, (2, 0)), None);
}

#[test]
fn before_the_release_a_change_keeps_the_version_and_after_it_takes_the_next() {
    let old = layout(&[("Fixture", shape!(Headed { header, value }))]);
    let grown = layout(&[(
        "Fixture",
        shape!(HeadedGrown {
            header,
            value,
            added
        }),
    )]);
    let changed = layout(&[("Fixture", shape!(Plain { value }))]);
    for new in [&grown, &changed, &old] {
        assert_eq!(wrong_change((1, 0), &old, new, (1, 0), false), None);
        for bumped in [(1, 1), (2, 0)] {
            assert!(
                wrong_change((1, 0), &old, new, bumped, false).is_some(),
                "an unreleased ABI keeps 1.0, not {bumped:?}"
            );
        }
    }
    for new in [&grown, &changed] {
        assert!(
            wrong_change((1, 0), &old, new, (1, 0), true).is_some(),
            "a released ABI needs a new version"
        );
    }
    assert_eq!(wrong_change((1, 0), &old, &grown, (1, 1), true), None);
    assert!(
        wrong_change((1, 0), &old, &born_at(grown.clone(), 12), (1, 0), false).is_some(),
        "an unreleased change is born at its size"
    );
    assert_eq!(wrong_change((1, 0), &old, &old, (1, 0), true), None);
}

#[test]
fn a_stable_major_steps_over_the_alpha_majors() {
    let old = layout(&[("Fixture", shape!(Headed { header, value }))]);
    let below = *ALPHA_MAJORS.start() - 1;
    let past = *ALPHA_MAJORS.end() + 1;
    let why = wrong_bump((below, 3), &old, &old, (below + 1, 0)).unwrap();
    assert!(why.contains(&format!("is {past}")), "{why}");
    assert_eq!(wrong_bump((below, 3), &old, &old, (past, 0)), None);
}

#[test]
fn an_append_keeps_what_readers_require_of_older_writers() {
    let old = born_at(layout(&[("Fixture", shape!(Headed { header, value }))]), 12);
    let grown = layout(&[(
        "Fixture",
        shape!(HeadedGrown {
            header,
            value,
            added
        }),
    )]);
    assert_eq!(
        wrong_bump((5, 2), &old, &born_at(grown.clone(), 12), (5, 3)),
        None
    );
    let moved = wrong_bump((5, 2), &old, &born_at(grown.clone(), 16), (5, 3)).unwrap();
    assert!(
        moved.contains("impl_headed!(Fixture, born = 12)"),
        "{moved}"
    );
    assert_eq!(
        wrong_bump((5, 2), &old, &born_at(grown.clone(), 16), (6, 0)),
        None
    );
    assert!(
        wrong_bump((5, 2), &old, &born_at(grown, 12), (6, 0)).is_some(),
        "a new major starts again at the size"
    );
}

#[test]
fn a_type_that_becomes_headed_in_an_append_is_born_at_its_old_size() {
    let old = layout(&[("Fixture", shape!(Headed { header, value }))]);
    let grown = layout(&[(
        "Fixture",
        shape!(HeadedGrown {
            header,
            value,
            added
        }),
    )]);
    assert!(wrong_bump((5, 2), &old, &born_at(grown.clone(), 16), (5, 3)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &born_at(grown, 12), (5, 3)), None);
}

#[test]
fn a_type_stays_headed_for_the_rest_of_its_major() {
    let headed = || layout(&[("Fixture", shape!(Headed { header, value }))]);
    let old = born_at(headed(), 12);
    assert!(wrong_bump((5, 2), &old, &headed(), (5, 2)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &headed(), (6, 0)), None);
}

#[test]
fn the_born_sizes_are_those_of_the_headed_types() {
    let layout = current();
    assert!(
        layout.born.len() > 50,
        "{} headed types found",
        layout.born.len()
    );
    for name in layout.born.keys() {
        let first = layout.types[name].fields.first();
        assert!(first.is_some_and(|field| field.ty == "Header"), "{name}");
    }
}

#[test]
fn a_new_type_needs_the_next_minor() {
    let old = layout(&[("Headed", shape!(Headed { header, value }))]);
    let new = layout(&[
        ("Headed", shape!(Headed { header, value })),
        ("Plain", shape!(Plain { value })),
    ]);
    assert_eq!(wrong_bump((5, 2), &old, &new, (5, 3)), None);
    assert!(wrong_bump((5, 2), &old, &new, (5, 2)).is_some());
}

#[test]
fn a_renumbered_constant_needs_a_new_major_and_a_new_one_the_next_minor() {
    let with = |pairs: &[(&str, &str)]| {
        let mut snapshot = layout(&[("Headed", shape!(Headed { header, value }))]);
        snapshot.constants = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        snapshot
    };
    let old = with(&[("Status::OK", "0")]);
    let renumbered = with(&[("Status::OK", "1")]);
    assert!(wrong_bump((5, 2), &old, &renumbered, (5, 3)).is_some());
    assert!(wrong_bump((5, 2), &old, &renumbered, (5, 2)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &renumbered, (6, 0)), None);
    let added = with(&[("Status::OK", "0"), ("Status::BUSY", "16")]);
    assert_eq!(wrong_bump((5, 2), &old, &added, (5, 3)), None);
    assert!(wrong_bump((5, 2), &old, &added, (5, 2)).is_some());
    assert!(
        wrong_bump((5, 2), &old, &with(&[]), (5, 3)).is_some(),
        "a removed constant breaks"
    );
}

#[test]
fn every_constant_of_the_abi_has_its_value_in_the_golden_file() {
    let constants = constants_in_source();
    assert_eq!(constants.get("Status::OK").map(String::as_str), Some("0"));
    assert_eq!(constants.get("Side::BOTH").map(String::as_str), Some("3"));
    assert!(
        !constants.contains_key("ABI_MAJOR"),
        "the version is no contract value"
    );
    assert!(constants.len() > 300, "{} constants found", constants.len());
}

#[test]
fn a_removed_field_needs_a_new_major() {
    let old = || {
        shape!(HeadedGrown {
            header,
            value,
            added
        })
    };
    let new = || shape!(Headed { header, value });
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
    assert!(!fits(old(), new(), (6, 1)), "a new major starts at minor 0");
}

#[test]
fn a_removed_type_needs_a_new_major() {
    let old = layout(&[
        ("Headed", shape!(Headed { header, value })),
        ("Plain", shape!(Plain { value })),
    ]);
    let new = layout(&[("Headed", shape!(Headed { header, value }))]);
    assert!(wrong_bump((5, 2), &old, &new, (5, 3)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &new, (6, 0)), None);
}

#[test]
fn a_struct_without_a_header_grows_only_in_a_new_major() {
    let old = || shape!(Plain { value });
    let new = || shape!(PlainGrown { value, added });
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
}

#[test]
fn a_changed_function_signature_needs_a_new_major() {
    assert_eq!(size_of::<Before>(), size_of::<After>());
    let old = || shape!(Before { call });
    let new = || shape!(After { call });
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
}

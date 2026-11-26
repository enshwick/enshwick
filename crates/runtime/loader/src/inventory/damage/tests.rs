//! The damage filters' tests: groups, sites and the hooks' decisions.

use super::*;

#[test]
fn amounts_left_negative_or_not_finite_count_as_none() {
    assert!((kept(-3.0)).abs() < f32::EPSILON);
    assert!((kept(f32::NAN)).abs() < f32::EPSILON);
    assert!((kept(2.5) - 2.5).abs() < f32::EPSILON);
}

/// No branch of a site's function lands inside the bytes its hook's
/// jump replaces, so every site arms, on each executable
/// `ENSH_SERVER_EXE`, `ENSH_CLIENT_EXE` and the opt-in `ENSH_EXES` path
/// list name, a server when its file is `enshrouded_server.exe`.
#[test]
#[ignore = "needs ENSH_SERVER_EXE and ENSH_CLIENT_EXE"]
fn every_site_arms_on_the_game_executables() {
    use enshwick_gamedata::reflect::{Image, Layout};
    use enshwick_gamedata::{Side, exact};

    let tables = crate::engine::builds::load();
    let list = std::env::var_os("ENSH_EXES");
    let paths = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
        .into_iter()
        .map(|var| {
            std::env::var_os(var).map_or_else(
                || panic!("{var} names the game executable"),
                std::path::PathBuf::from,
            )
        })
        .chain(list.iter().flat_map(std::env::split_paths));
    for path in paths {
        let server = path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("enshrouded_server.exe"));
        let side = if server { Side::Server } else { Side::Client };
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let Some(table) = exact(&tables, side, image.fingerprint().unwrap()) else {
            println!(
                "{}: no table is made for this build; skipped",
                path.display()
            );
            continue;
        };
        let resolved = table.resolve();
        let pdata = image.range(image.section(".pdata").unwrap());
        let rows = image.read(pdata.start, pdata.end - pdata.start).unwrap();
        let base = image.base();
        for &(symbol, _) in GROUPS.iter().flat_map(|group| group.sites) {
            let Some(rva) = resolved.symbol(symbol.id()) else {
                continue;
            };
            let (begin, end) = rows
                .as_chunks::<12>()
                .0
                .iter()
                .map(|row| {
                    let at =
                        |i: usize| u64::from(u32::from_le_bytes(row[i..i + 4].try_into().unwrap()));
                    (at(0), at(4))
                })
                .find(|&(begin, end)| (begin..end).contains(&rva))
                .unwrap();
            let code = image.read(base + begin, end - begin).unwrap();
            let branch = crate::engine::mid::branch_into_hook(code, base + begin, base + rva);
            assert_eq!(
                branch.map(|at| at - base),
                None,
                "{}: {symbol} at {rva:#x}",
                path.display()
            );
        }
    }
}

#[test]
fn a_hit_no_filter_or_stat_watches_is_left_unread() {
    let _first = STATE.layout.set(Layout {
        vector: 0x40,
        target: 0x10,
        source: 0x14,
    });
    // No page maps the frame this names: a read of it faults the test.
    let mut registers = Registers {
        rbp: 0x20,
        ..Registers::default()
    };
    // SAFETY: the registers are borrowed for the call, as a stub's are.
    unsafe { hit(&raw mut registers, core::ptr::null_mut()) };
    assert!(!HIT_FUSE.is_off());
}

unsafe extern "system" fn keep(_check: *mut DamageCheck, _user: *mut c_void) {}

/// Filters holding one filter of mod 0, sealed as the seal phase does.
fn one_filter() -> Filters<DamageFilterFn> {
    let filters = Filters::<DamageFilterFn>::new(Feature::Damage);
    let added = filters.register(ModIndex::new(0), keep, core::ptr::null_mut());
    assert!(added.is_ok());
    let _sealed = filters.seal();
    filters
}

/// A server's loader whose build pins every site but `lacking` and,
/// with `layout`, the hit's frame layout.
fn loader_without(lacking: Option<Symbol>, layout: bool) -> Loader {
    let sites: Vec<Symbol> = GROUPS
        .iter()
        .flat_map(|group| group.sites)
        .map(|&(site, _)| site)
        .filter(|&site| Some(site) != lacking)
        .collect();
    let constants = [
        pinned::damage::HIT_VECTOR,
        pinned::damage::HIT_TARGET,
        pinned::damage::HIT_SOURCE,
    ]
    .map(|constant| (constant, 0x10));
    crate::start::tests::pinning_loader(
        enshwick_core::abi::Side::SERVER,
        &sites,
        if layout { &constants } else { &[] },
    )
}

/// The groups' switches that are off, by name.
fn off() -> Vec<&'static str> {
    GROUPS
        .iter()
        .filter(|group| group.fuse.is_off())
        .map(|group| group.fuse.feature())
        .collect()
}

#[test]
fn a_build_whose_fall_death_site_does_not_route_turns_off_only_the_falls() {
    let filters = one_filter();
    let loader = loader_without(Some(pinned::damage::FALL_DEATH), true);
    link_in(&loader, filters.sealed().unwrap_or_default(), false);
    let fall = crate::base::off_reason(&FALL_FUSE);
    assert!(
        fall.as_deref()
            .is_some_and(|r| r.starts_with(pinned::damage::FALL_DEATH.id())),
        "{fall:?}"
    );
    assert!(
        FALL_FUSE.feature().contains("(FallDamage)"),
        "the off line names the cause"
    );
    // The bare reflection lacks the damage types, so the hits go for
    // their own reason.
    let hit = crate::base::off_reason(&HIT_FUSE);
    assert!(
        hit.as_deref()
            .is_some_and(|r| r.starts_with(catalogue::type_name(catalogue::ids::DAMAGE_TYPE))),
        "{hit:?}"
    );
    assert_eq!(off(), [HIT_FUSE.feature(), FALL_FUSE.feature()]);
    let shroud = loader.address(pinned::damage::SHROUD).unwrap_or_default();
    let linked = enshwick_base::Relock::relock(&loader.mids).features_at(shroud);
    assert!(
        SHROUD_FUSE.among(&linked),
        "the shroud site links after the falls went off"
    );
}

#[test]
fn a_build_without_the_hit_layout_turns_off_only_the_hits() {
    let filters = one_filter();
    link_in(
        &loader_without(None, false),
        filters.sealed().unwrap_or_default(),
        false,
    );
    let hit = crate::base::off_reason(&HIT_FUSE);
    assert!(
        hit.as_deref()
            .is_some_and(|r| r.starts_with(pinned::damage::HIT_VECTOR.id())),
        "{hit:?}"
    );
    assert_eq!(off(), [HIT_FUSE.feature()]);
}

#[test]
fn a_build_without_any_site_turns_every_group_off_by_its_own_switch() {
    let filters = one_filter();
    link_in(
        &crate::start::tests::bare_loader(),
        filters.sealed().unwrap_or_default(),
        false,
    );
    assert_eq!(off().len(), GROUPS.len());
}

#[test]
fn nothing_filtering_damage_links_no_site_and_leaves_the_filters_on() {
    link_in(&crate::start::tests::bare_loader(), &[], false);
    assert!(off().is_empty());
}

#[test]
fn stats_alone_link_only_the_hit_site_and_filters_link_all_ten() {
    let ids = |filtering| {
        groups(filtering)
            .iter()
            .flat_map(|group| group.sites)
            .map(|(s, _)| s.id())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(false), [pinned::damage::HIT.id()]);
    assert_eq!(ids(true).len(), 10);
}

#[test]
fn every_damage_cause_is_served_by_exactly_one_group_whose_switch_names_it() {
    for cause in DamageCause::NAMED {
        let groups = GROUPS
            .iter()
            .filter(|group| group.causes.contains(cause))
            .count();
        assert_eq!(groups, 1, "{cause:?}");
    }
    let all: usize = GROUPS.iter().map(|group| group.causes.len()).sum();
    assert_eq!(all, DamageCause::NAMED.len(), "no cause twice");
    for group in &GROUPS {
        let listed: Vec<String> = group.causes.iter().map(|c| format!("{c:?}")).collect();
        let named = format!("({})", listed.join(", "));
        assert!(
            group.fuse.feature().ends_with(&named),
            "{}",
            group.fuse.feature()
        );
    }
}

#[test]
fn a_build_without_the_fall_sites_answers_only_the_falls_off_to_a_filter() {
    let filters = one_filter();
    let loader = loader_without(Some(pinned::damage::FALL_DEATH), true);
    link_in(&loader, filters.sealed().unwrap_or_default(), false);
    // The bare reflection lacks the damage types, so the hits are off too.
    let expected: Vec<u32> = [DamageCause::Impact, DamageCause::FallDamage]
        .iter()
        .map(|cause| cause.to_abi())
        .collect();
    assert_eq!(off_of(true, false), expected);
}

#[test]
fn without_a_filter_every_cause_is_off_but_the_hits_stats_arm() {
    let named = DamageCause::NAMED.len();
    assert_eq!(off_of(false, false).len(), named);
    let with_stats = off_of(false, true);
    assert_eq!(with_stats.len(), named - 1);
    assert!(!with_stats.contains(&DamageCause::Impact.to_abi()));
    assert!(off_of(true, false).is_empty(), "nothing faulted");
}

// The sealed filters and the end of arming are process-wide, and the start
// tests seal and settle them too, so these tests read the answer apart from
// them.
#[test]
fn nothing_answers_before_the_filters_are_sealed() {
    assert_eq!(answer(true, None, false), None);
    assert_eq!(answer(true, None, true), None);
}

#[test]
fn nothing_answers_between_the_link_and_the_end_of_arming() {
    assert_eq!(
        answer(false, Some(false), false),
        None,
        "a switch may still go off while arming"
    );
    assert_eq!(answer(false, Some(true), true), None);
    assert_eq!(
        answer(true, Some(false), false).map(|off| off.len()),
        Some(DamageCause::NAMED.len())
    );
    assert_eq!(answer(true, Some(true), false), Some(Vec::new()));
}

/// `rflags` after `comiss` of a survivable fall (above) and a lethal
/// one (zero).
const ABOVE: u64 = 0x202;
const LETHAL: u64 = 0x202 | ZERO;

/// Whether `ja` jumps past the death record under `rflags`.
fn skips_the_death(rflags: u64) -> bool {
    rflags & (CARRY | ZERO) == 0
}

const ROW: Entity = Entity::from_engine(7);

#[test]
fn a_fall_a_filter_makes_lethal_records_the_death() {
    assert_eq!(fall_kept(ROW, 30, 10, |damage| damage * 3), 0);
    let written = WRITTEN.take();
    assert_eq!(written, Some((ROW, 0)));
    assert!(!skips_the_death(decided(written, ROW, ABOVE)));
}

#[test]
fn a_fall_a_filter_makes_lethal_where_the_game_already_wrote_0_records_the_death() {
    // Health 100 and a fall worked out to 0.4: the game writes 0 but
    // its float says alive.
    assert_eq!(fall_kept(ROW, 100, 0, |damage| damage + 50), 0);
    let written = WRITTEN.take();
    assert_eq!(written, Some((ROW, 0)));
    assert!(!skips_the_death(decided(written, ROW, ABOVE)));
}

#[test]
fn a_lethal_fall_a_filter_spares_records_no_death() {
    assert_eq!(fall_kept(ROW, 30, 0, |_| 0), 30);
    let written = WRITTEN.take();
    assert_eq!(written, Some((ROW, 30)));
    assert!(skips_the_death(decided(written, ROW, LETHAL)));
}

#[test]
fn a_fall_the_filters_leave_alone_or_that_takes_nothing_clears_the_note() {
    let _lethal = fall_kept(ROW, 30, 10, |damage| damage * 3);
    assert_eq!(
        fall_kept(Entity::from_engine(8), 30, 40, |_| unreachable!()),
        40
    );
    assert_eq!(WRITTEN.take(), None, "took nothing");
    let _lethal = fall_kept(ROW, 30, 10, |damage| damage * 3);
    assert_eq!(
        fall_kept(Entity::from_engine(8), 30, 10, |damage| damage),
        10
    );
    assert_eq!(WRITTEN.take(), None, "left alone");
}

#[test]
fn a_note_of_another_row_or_none_leaves_the_game_s_decision() {
    assert_eq!(
        decided(Some((Entity::from_engine(8), 30)), ROW, LETHAL),
        LETHAL
    );
    assert_eq!(
        decided(Some((Entity::from_engine(8), 0)), ROW, ABOVE),
        ABOVE
    );
    assert_eq!(decided(None, ROW, LETHAL), LETHAL);
}

#[test]
fn a_filtered_write_keeps_what_the_filter_leaves_and_never_goes_below_zero() {
    assert_eq!(kept_health(30, 10, |damage| damage / 2), 20);
    assert_eq!(kept_health(30, 10, |damage| damage * 3), 0);
    assert_eq!(kept_health(30, 30, |_| unreachable!()), 30, "no damage");
}

#[test]
fn without_filters_damage_stays() {
    assert_eq!(filtered(DamageCheck::FALL_DAMAGE, 1, 40), 40);
    assert!(kill_allowed(1));
}

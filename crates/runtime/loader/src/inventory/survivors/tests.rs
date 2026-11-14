use super::menu::file;
use super::*;

#[test]
fn the_survivor_lookup_is_wholly_off_when_one_of_its_links_fails() {
    let loader = crate::start::tests::bare_loader();
    assert!(!lookup::link(&loader), "this build has no lookup");
    assert!(LOOKUP_FUSE.is_off());
    assert!(
        !moves::link_moves(&loader),
        "the moves and updates of an off lookup are not linked"
    );
    let mut chains = loader.chains.relock();
    chains.seal();
    assert!(chains.is_empty());
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == "survivor lookup");
    assert!(
        off.is_some_and(|p| p.reason.starts_with(pinned::npc::LOOKUP.id())),
        "{listed:?}"
    );
}

#[test]
fn a_survivor_feature_no_mod_asked_for_links_nothing_and_stays_on() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
    link(loader);
    let fuses = [
        LOOKUP_FUSE.is_off(),
        BEDS_FUSE.is_off(),
        STAFFS_FUSE.is_off(),
    ];
    assert_eq!(fuses, [false; 3]);
    assert!(!FILED_FUSE.is_off() && !MENU_FUSE.is_off());
    assert!(loader.chains.relock().is_empty());
    assert!(STATE.registrations.is_sealed(), "the link seals them");
    STATE.registrations.reopen();
    assert!(
        !STATE.registrations.is_sealed(),
        "the next test registers again"
    );
}

unsafe extern "system" fn no_lookup(
    _call: *const enshwick_core::abi::SurvivorLookup,
    _user: *mut c_void,
    _out: *mut enshwick_core::Entity,
) -> u8 {
    0
}

#[test]
fn one_mod_holds_the_lookup_until_the_seal_and_a_reopened_one_takes_any() {
    let registrations = Registrations::new();
    let (first, second) = (ModIndex::new(1), ModIndex::new(2));
    let hold = |owner| {
        Registrations::hold_one(
            &registrations.lookup,
            Registered::new(owner, no_lookup, core::ptr::null_mut()),
        )
    };
    assert_eq!(hold(first), Status::OK);
    assert_eq!(hold(first), Status::OK, "the same mod replaces its own");
    assert_eq!(hold(second), Status::REFUSED);
    registrations.seal();
    assert!(registrations.is_sealed());
    assert_eq!(registrations.lookup().map(|r| r.owner), Some(first));
    assert_eq!(hold(first), Status::TOO_LATE);
    registrations.reopen();
    assert!(!registrations.is_sealed());
    assert_eq!(hold(second), Status::OK);
}

#[test]
fn a_hosted_world_s_end_forgets_its_component_types_and_its_move() {
    let state = State::new();
    state.transform_type.store(3, Ordering::Release);
    state.npc_common_type.store(4, Ordering::Release);
    assert!(state.knows_types());
    state.start_move(7, 9);
    assert!(state.move_active.load(Ordering::Acquire));
    state.forget();
    assert!(
        !state.knows_types(),
        "the next world's indexes are read anew"
    );
    assert!(state.moving.relock().is_none());
    assert!(!state.move_active.load(Ordering::Acquire));
}

#[test]
fn a_build_whose_menu_system_does_not_read_lists_the_survivor_menu_off() {
    let loader = crate::start::tests::bare_loader();
    assert!(menu_system(&loader).is_none());
    assert!(MENU_FUSE.is_off(), "off by its fuse, not only in the log");
}

#[test]
fn filing_marks_a_free_slot_and_overwrites_a_used_one() {
    unsafe extern "system" fn find(_map: *mut u8, key: *const u32) -> u64 {
        // SAFETY: `file` passes a pointer to its own `u32` item.
        u64::from(unsafe { key.read() } & 127)
    }
    let mut bits = [0u64; 2];
    let mut keys = [0u32; 128];
    let mut values = [0u32; 128];
    let mut table = [0u8; 0x60];
    table[0x08..0x10].copy_from_slice(&(bits.as_mut_ptr() as u64).to_le_bytes());
    table[0x10..0x18].copy_from_slice(&128u64.to_le_bytes());
    table[0x28..0x30].copy_from_slice(&(keys.as_mut_ptr() as u64).to_le_bytes());
    table[0x40..0x48].copy_from_slice(&(values.as_mut_ptr() as u64).to_le_bytes());
    let map = ItemMap {
        find_slot: find,
        loaded: 0x20,
        at: 0,
        bits: 0x08,
        capacity: 0x10,
        keys: 0x28,
        values: 0x40,
        count: 0x54,
    };
    // SAFETY: `table` is laid out at `map`'s offsets over the test's own
    // 128-slot bitset, keys and values, `find` is its slot search, and
    // only this thread uses them.
    assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 5) });
    // SAFETY: `table` is laid out at `map`'s offsets over the test's own
    // 128-slot bitset, keys and values, `find` is its slot search, and
    // only this thread uses them.
    assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 6) });
    assert_eq!(bits[1], 1 << 6);
    assert_eq!((keys[70], values[70]), (70, 6));
    assert_eq!(u32::from_le_bytes(table[0x54..0x58].try_into().unwrap()), 1);
}

use enshwick_core::abi::Event;

use super::{LOADED, LOADING, UNLOADED, UNLOADING, as_saved, remapped, turn};

#[test]
fn only_a_finished_load_or_unload_of_a_chunk_raises_its_event() {
    assert_eq!(turn(LOADING, LOADED), Some(Event::CHUNK_LOADED));
    assert_eq!(turn(UNLOADING, UNLOADED), Some(Event::CHUNK_UNLOADED));
    for (before, after) in [
        (LOADING, LOADING),
        (LOADED, LOADED),
        (LOADED, UNLOADING),
        (UNLOADING, UNLOADING),
        (UNLOADED, LOADING),
        (UNLOADED, UNLOADED),
        (0xff, LOADED),
    ] {
        assert_eq!(turn(before, after), None, "{before} to {after}");
    }
}

const A: [u8; 16] = [1; 16];
const B: [u8; 16] = [2; 16];
const X: [u8; 16] = [9; 16];
const C: [u8; 16] = [3; 16];

#[test]
fn a_save_without_its_phenomena_reads_as_the_running_world_s_so_fronts_past_a_layer_are_freed() {
    let served = [A, B];
    assert_eq!(
        as_saved(None, &served),
        served,
        "a save from before the file"
    );
    assert_eq!(
        as_saved(Some(Vec::new()), &served),
        served,
        "written while they did not read"
    );
    assert_eq!(as_saved(Some(vec![A, X]), &served), [A, X]);
    let saved = as_saved(None, &served);
    assert_eq!(
        remapped(&saved, &served, (0, 2), 1),
        Some(1),
        "a vanilla front kept"
    );
    assert_eq!(
        remapped(&saved, &served, (0, 2), 2),
        None,
        "a gone mod's front freed"
    );
}

#[test]
fn a_saved_front_follows_its_phenomenon_s_guid_to_its_index_now() {
    let (saved, served) = ([A, B, X, C], [A, B, C]);
    let all = (0, 3);
    assert_eq!(remapped(&saved, &served, all, 3), Some(2), "C moved down");
    assert_eq!(remapped(&saved, &served, all, 2), None, "X is gone");
    assert_eq!(remapped(&saved, &served, all, 0), Some(0));
    assert_eq!(
        remapped(&saved, &served, all, 4),
        None,
        "past the saved list"
    );
}

#[test]
fn a_phenomenon_outside_its_layer_s_range_is_freed() {
    let (saved, served) = ([A, B, C], [A, B, C]);
    assert_eq!(remapped(&saved, &served, (0, 2), 2), None);
    assert_eq!(remapped(&saved, &served, (1, 2), 2), Some(2));
}

#[test]
fn a_layer_whose_range_reaches_the_last_index_keeps_its_front_there() {
    let all: Vec<[u8; 16]> = (0..=255u8).map(|i| [i; 16]).collect();
    assert_eq!(remapped(&all, &all, (250, 6), 255), Some(255));
}

mod live {
    use super::super::Layout;
    use super::super::check::system_holds;

    unsafe extern "system" fn sample(
        out: *mut [f32; 4],
        _: *const u8,
        _: *const [i64; 3],
        _: i64,
    ) -> *mut [f32; 4] {
        out
    }
    unsafe extern "system" fn clock(_: *const u8) -> i64 {
        0
    }
    unsafe extern "system" fn lock(_: *mut [u64; 4], _: u64, _: u32) -> u8 {
        0
    }
    unsafe extern "system" fn release(_: u64, _: u32) {}
    unsafe extern "system" fn free(_: *mut u8, _: u32) {}
    unsafe extern "system" fn spawn(
        _: *mut u8,
        _: *const u8,
        _: i64,
        _: u64,
        _: u8,
        _: *const [u64; 2],
    ) {
    }

    /// A weather system laid out as words: the layer count, the layer array,
    /// the page count, the page array and the page shift; layer slots of
    /// four words.
    fn layout() -> Layout {
        Layout {
            sample,
            clock,
            lock,
            release,
            free,
            spawn,
            system: 0,
            game: 8,
            manager: 0,
            resource: 0,
            generation: 0,
            pages: 0x18,
            page_count: 0x10,
            shift: 0x20,
            mask: 0,
            page_size: 0,
            page_entries: 0,
            page_entry_count: 0,
            entry_size: 0,
            entry_generation: 0,
            entry_data: 0,
            layers: 8,
            layer_count: 0,
            slot_size: 0x20,
            slot_time: 0,
            slot_phenomenon: 8,
            slot_current: 0x10,
            slot_next: 0x14,
            instance_phenomenon: 0,
            instance_position: 0,
            instance_radius: 0,
            instance_despawn: 0,
            resource_layers: 0,
            resource_phenomena: 0,
            layer_size: 0,
            layer_range_start: 0,
            layer_range_count: 0,
            phenomenon_size: 0,
            phenomenon_effect: 0,
            phenomenon_guid: None,
        }
    }

    #[test]
    fn a_live_weather_system_holds_only_with_readable_layers_and_bounded_counts() {
        let layout = layout();
        let slots = [[0usize; 4]; 3];
        let pages = [0usize; 2];
        let mut system = [3, slots.as_ptr().addr(), 2, pages.as_ptr().addr(), 6];
        let holds = |system: &[usize; 5]| {
            let server = [system.as_ptr().addr()];
            system_holds(&layout, server.as_ptr().addr())
        };
        assert_eq!(holds(&system), Ok(()));
        system[0] = 0;
        system[1] = 0;
        assert_eq!(holds(&system), Ok(()), "no layers, no array to read");
        system[0] = 300;
        let past = holds(&system).unwrap_err();
        assert_eq!(past.entry, crate::pinned::weather::LAYER_COUNT.id());
        system[0] = 3;
        system[1] = 0x10;
        let dangling = holds(&system).unwrap_err();
        assert_eq!(dangling.entry, crate::pinned::weather::LAYERS.id());
        system[1] = slots.as_ptr().addr();
        system[4] = 70;
        let shift = holds(&system).unwrap_err();
        assert_eq!(shift.entry, crate::pinned::weather::PAGE_SHIFT.id());
        assert!(
            system_holds(&layout, 0x10).is_err(),
            "a server that does not read"
        );
    }

    #[test]
    fn a_page_array_is_read_only_while_pages_exist_and_a_shift_stays_under_64() {
        let layout = layout();
        let slots = [[0usize; 4]; 3];
        let mut system = [3, slots.as_ptr().addr(), 0, 0x10, 63];
        let holds = |system: &[usize; 5]| {
            let server = [system.as_ptr().addr()];
            system_holds(&layout, server.as_ptr().addr())
        };
        assert_eq!(holds(&system), Ok(()), "no pages, no array to read");
        system[2] = 1;
        let dangling = holds(&system).unwrap_err();
        assert_eq!(dangling.entry, crate::pinned::weather::PAGES.id());
        system[2] = 0;
        system[4] = 64;
        let shift = holds(&system).unwrap_err();
        assert_eq!(shift.entry, crate::pinned::weather::PAGE_SHIFT.id());
    }
}

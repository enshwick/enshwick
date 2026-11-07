use enshwick::abi::Event;

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

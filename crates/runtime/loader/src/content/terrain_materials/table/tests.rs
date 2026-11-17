use proptest::prelude::*;

use super::*;
use crate::content::terrain_materials::place;
use crate::content::terrain_materials::tests::{MOD, ROCK, SAND, guid, palette, request};

/// The material of mod `owner` at `id` with GUID `n`, like sand, falling
/// back to rock.
fn material(owner: ModIndex, n: u8, id: u8) -> Material {
    let mut request = request(n);
    request.fallback = ROCK;
    Material {
        owner,
        id,
        ..place(&palette(), &[], owner, &request).unwrap()
    }
}

fn entry(id: u8, n: u8, fallback: u8) -> Entry {
    Entry {
        id,
        guid: guid(n),
        fallback,
    }
}

#[test]
fn a_hosts_table_reads_back_as_it_was_written() {
    let served = [
        material(MOD, 150, 117),
        material(ModIndex::LOADER, 151, 118),
    ];
    let read = parse(&text(&served), &palette()).unwrap();
    assert_eq!(read, [entry(117, 150, ROCK), entry(118, 151, ROCK)]);
    assert_eq!(parse("", &palette()), Some(Vec::new()), "a host with none");
}

#[test]
fn a_table_naming_anything_but_free_ids_new_guids_and_game_fallbacks_is_none() {
    let palette = palette();
    let good = format!("117:{}:9", guid(150));
    assert!(parse(&good, &palette).is_some());
    let bad = [
        format!("116:{}:9", guid(150)),
        format!("128:{}:9", guid(150)),
        format!("117:{}:9", guid(SAND)),
        format!("117:{}:9", Guid::NIL),
        format!("117:{}:117", guid(150)),
        format!("117:{}:0", guid(150)),
        format!("{good},117:{}:9", guid(151)),
        format!("{good},118:{}:9", guid(150)),
        format!("{good}:1"),
        format!("{good},"),
        "117:not-a-guid:9".to_owned(),
        "x".to_owned(),
    ];
    for text in bad {
        assert_eq!(parse(&text, &palette), None, "{text}");
    }
}

#[test]
fn a_table_of_more_entries_than_free_ids_is_none() {
    let twelve: Vec<String> = (0..12u8)
        .map(|i| format!("{}:{}:9", 117 + i % 11, guid(150 + i)))
        .collect();
    assert_eq!(parse(&twelve.join(","), &palette()), None);
}

#[test]
fn a_removed_mods_ground_the_client_never_had_takes_the_hosts_id_and_fallback() {
    let adopted = adopt(&[], &[entry(119, 150, ROCK)], |_| true);
    assert_eq!((adopted.table.len(), adopted.moved), (1, None));
    let orphan = adopted.table[0];
    assert_eq!(
        (orphan.id, orphan.guid, orphan.like),
        (119, guid(150), ROCK)
    );
    assert_eq!(orphan.owner, ModIndex::LOADER);
}

#[test]
fn a_running_mods_material_at_the_hosts_id_keeps_its_rules() {
    let own = [
        material(MOD, 150, 117),
        material(ModIndex::LOADER, 160, 118),
    ];
    let host = [entry(117, 150, ROCK), entry(118, 161, SAND)];
    let adopted = adopt(&own, &host, |_| true).table;
    assert_eq!(adopted[0], own[0], "the mod's own material");
    assert_eq!(
        (adopted[1].guid, adopted[1].like),
        (guid(161), SAND),
        "the host's orphan, not this side's"
    );
}

#[test]
fn a_running_mod_the_host_serves_elsewhere_or_not_at_all_is_named() {
    let own = [material(MOD, 150, 118)];
    let elsewhere = adopt(&own, &[entry(117, 150, ROCK)], |_| true);
    assert_eq!(elsewhere.moved, Some(MOD));
    assert_eq!(
        (elsewhere.table[0].id, elsewhere.table[0].owner),
        (117, MOD),
        "the host's id, with the mod's rules"
    );
    assert_eq!(
        adopt(&own, &[entry(118, 151, ROCK)], |_| true).moved,
        Some(MOD)
    );
}

#[test]
fn a_mod_turned_off_for_the_world_gives_its_id_to_the_host() {
    let extra = ModIndex::new(2);
    let own = [material(extra, 170, 117), material(MOD, 150, 118)];
    let host = [entry(117, 150, ROCK)];
    let off_extra = |owner: ModIndex| owner != extra;
    assert_eq!(
        adopt(&own, &host, off_extra).moved,
        Some(MOD),
        "the shifted mod"
    );
    let own = [material(extra, 170, 118), material(MOD, 150, 117)];
    let adopted = adopt(&own, &host, off_extra);
    assert_eq!(
        adopted,
        Adopted {
            table: vec![own[1]],
            moved: None
        }
    );
}

proptest! {
    #[test]
    fn any_text_reads_without_a_panic(text in ".{0,600}") {
        drop(parse(&text, &palette()));
    }

    #[test]
    fn any_table_of_free_ids_reads_back(ids in proptest::sample::subsequence((117u8..=127).collect::<Vec<_>>(), 0..=11)) {
        let served: Vec<Material> = ids
            .iter()
            .map(|&id| material(MOD, id + 20, id))
            .collect();
        let read = parse(&text(&served), &palette()).unwrap();
        prop_assert_eq!(read.len(), served.len());
        prop_assert!(read.iter().zip(&served).all(|(e, m)| e.id == m.id && e.guid == m.guid));
    }
}

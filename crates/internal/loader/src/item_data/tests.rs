//! Tests of kind ids, destroy requests, the client's cap and the world file's records.

use super::attach::attach;
use super::world_file::{read_file, read_world, unwritten, write_file};
use super::*;
use crate::item_records::anchor::Index;
use crate::item_records::file::{self, Saved};

#[test]
fn ids_take_the_mod_s_namespace_unless_they_name_one() {
    assert_eq!(
        full_id("you.lanterns", "oil").as_deref(),
        Some("you.lanterns:oil")
    );
    assert_eq!(
        full_id("you.lanterns", "other:fire_damage").as_deref(),
        Some("other:fire_damage")
    );
    assert_eq!(full_id("you.lanterns", "Charges"), None);
    assert_eq!(full_id("you.lanterns", "a:b:c"), None);
    assert_eq!(full_id("you.lanterns", ":x"), None);
}

#[test]
fn a_destroyed_entity_loses_its_records_and_an_unloaded_one_keeps_them() {
    let record = |item| Record {
        item,
        anchor: None,
        value: Box::new([1]),
    };
    let mut values = Values::default();
    values.insert((0, 900), record(7));
    values.insert((1, 900), record(0));
    values.insert((0, 901), record(7));
    note_destroyed(Entity(900));
    note_destroyed(Entity(0));
    let destroyed = take_destroyed();
    assert!(take_destroyed().is_empty(), "each request is taken once");
    let mut gone = prune_destroyed(&mut values, &destroyed);
    gone.sort_unstable();
    assert_eq!(gone, [(0, 900), (1, 900)]);
    // 901 is merely not loaded: no destroy request names it.
    assert_eq!(values.len(), 1);
    assert!(values.contains_key(&(0, 901)));
    // Past the cap in one step, requests are left for the save.
    for entity in 1..=u32::try_from(DESTROYED_CAP).unwrap() + 10 {
        note_destroyed(Entity(entity));
    }
    let destroyed = take_destroyed();
    assert_eq!(destroyed.len(), DESTROYED_CAP);
    assert!(!destroyed.contains(&(u32::try_from(DESTROYED_CAP).unwrap() + 1)));
    // A request filled after its round was taken never reaches a later
    // one, where its slot is still being filled.
    note_destroyed(Entity(1));
    let late = reserve_destroyed();
    assert_eq!(take_destroyed().into_iter().collect::<Vec<_>>(), [1]);
    fill_destroyed(late, Entity(5));
    note_destroyed(Entity(2));
    let _filling = reserve_destroyed();
    assert_eq!(take_destroyed().into_iter().collect::<Vec<_>>(), [2]);
}

/// The kinds these tests share, sealed once per process: `a:charges`
/// (0), an item kind clients get, and `a:owner` (1), an entity kind.
fn kinds() {
    let _charges = KINDS.add(Kind {
        id: "a:charges".into(),
        scope: Scope::Item,
        sent: true,
    });
    let _owner = KINDS.add(Kind {
        id: "a:owner".into(),
        scope: Scope::Entity,
        sent: false,
    });
    let _sealed = KINDS.seal();
}

#[test]
fn a_client_keeps_at_most_its_cap_of_a_server_s_records() {
    kinds();
    let mut values = Values::default();
    let mut bytes = Vec::new();
    for pide in 1..=u32::try_from(MAX_CLIENT_RECORDS).unwrap() {
        encode(&mut bytes, "a:charges", pide, 7, &[1]);
    }
    assert!(keep(&mut values, &bytes));
    assert_eq!(values.len(), MAX_CLIENT_RECORDS);
    let mut more = Vec::new();
    encode(&mut more, "a:charges", 1, 7, &[2]);
    encode(&mut more, "a:charges", u32::MAX, 7, &[1]);
    assert!(
        !keep(&mut values, &more),
        "a new record past the cap is dropped"
    );
    assert_eq!(values.len(), MAX_CLIENT_RECORDS);
    assert_eq!(&*values[&(0, 1)].value, &[2], "a known one still changes");
    let mut removal = Vec::new();
    encode(&mut removal, "a:charges", 1, 0, &[]);
    assert!(keep(&mut values, &removal));
    assert_eq!(values.len(), MAX_CLIENT_RECORDS - 1);
}

fn server(pending: Vec<Pending>, unopened: Vec<String>) -> Server {
    Server {
        key: Some("w".into()),
        values: Values::default(),
        pending,
        unopened,
        changed: false,
    }
}

fn saved<'a>(id: &'a str, anchor: Anchor, value: &'static [u8]) -> Saved<'a> {
    Saved {
        id,
        anchor,
        check: 9,
        value,
        aside: false,
    }
}

fn chest(x: u8) -> anchor::Placed {
    anchor::Placed::new([1; 16], [i64::from(x) << 32; 3])
}

/// Attaches `server`'s pending records against `index`, as the first
/// step after a load does.
fn first_step(server: &mut Server, index: &Index) {
    let _attached = attach(server, Some(index), |_, _, _| None);
}

#[test]
fn a_world_that_does_not_read_leaves_every_record_as_read() {
    kinds();
    let bytes = file::write([
        saved("a:owner", Anchor::Placed(chest(1)), &[5]),
        saved("gone:owner", Anchor::Placed(chest(2)), &[6]),
    ]);
    let (pending, unopened) = read_file(&bytes).unwrap();
    let mut world = server(pending.clone(), unopened);
    assert_eq!(attach(&mut world, None, |_, _, _| None), None);
    assert_eq!(world.pending, pending, "nothing went aside");
    assert!(world.values.is_empty());
    assert!(!world.changed, "nothing to write");
    assert_eq!(write_file(&world), bytes, "and if written, the same bytes");
}

#[test]
fn a_leaving_player_s_item_values_go_aside_and_no_other_value_moves() {
    const ANN: u32 = 7;
    const BEN: u32 = 8;
    const CHEST: u32 = 1046;
    kinds();
    let mut world = server(Vec::new(), Vec::new());
    let value = |item, anchor, byte| Record {
        item,
        anchor,
        value: Box::new([byte]),
    };
    // `a:charges` (0) on Ann's two items, Ben's item and a chest's item;
    // `a:owner` (1) on Ann's character itself.
    world.values.insert((0, 2001), value(77, None, 1));
    world.values.insert((0, 2002), value(78, None, 2));
    world.values.insert((0, 2003), value(77, None, 3));
    let in_chest = Some(Anchor::Held {
        root: chest(1),
        link: 0,
        slot: 3,
    });
    world.values.insert((0, 2004), value(77, in_chest, 4));
    world
        .values
        .insert((1, ANN), value(9, Some(Anchor::Placed(chest(2))), 5));
    let held = |pide| {
        let (root, item) = match pide {
            2001 => (ANN, 77),
            2002 => (ANN, 78),
            2003 => (BEN, 77),
            2004 => (CHEST, 77),
            _ => return None,
        };
        Some(game::Held {
            anchor: None,
            item,
            root,
        })
    };
    assert_eq!(set_aside_carried(&mut world, ANN, held), 2);
    let mut kept: Vec<(u32, u32)> = world.values.keys().copied().collect();
    kept.sort_unstable();
    assert_eq!(kept, [(0, 2003), (0, 2004), (1, ANN)]);
    let mut aside: Vec<(Anchor, u32, u8, bool)> = world
        .pending
        .iter()
        .map(|r| (r.anchor, r.check, r.value[0], r.aside))
        .collect();
    aside.sort_unstable();
    assert_eq!(
        aside,
        [
            (Anchor::Session(2001), 77, 1, true),
            (Anchor::Session(2002), 78, 2, true),
        ]
    );
    assert!(world.changed);
    // The items' later destroy requests find nothing left to drop.
    let mut destroyed = FxHashSet::default();
    destroyed.extend([2001, 2002]);
    assert!(prune_destroyed(&mut world.values, &destroyed).is_empty());
}

#[test]
fn records_of_kinds_no_mod_opened_follow_their_entities_until_the_mod_returns() {
    kinds();
    let bytes = file::write([
        saved("gone:owner", Anchor::Placed(chest(1)), &[3]),
        saved("gone:owner", Anchor::Placed(chest(2)), &[4]),
        saved("gone:empty", Anchor::Session(901), &[]),
    ]);
    let (pending, unopened) = read_file(&bytes).unwrap();
    assert_eq!(unopened, ["gone:owner"]);
    let mut world = server(pending, unopened);
    // Chest 2 is gone; chest 1 stands, renumbered.
    let mut index = Index::default();
    index.add(chest(1), 1050);
    first_step(&mut world, &index);
    assert_eq!(world.values.len(), 1);
    assert_eq!(
        world.kind_id(world.values.keys().next().unwrap().0),
        Some("gone:owner")
    );
    let written = write_file(&world);
    let records = file::read(&written).unwrap();
    assert_eq!(
        records,
        [
            saved("gone:owner", Anchor::Placed(chest(1)), &[3]),
            Saved {
                aside: true,
                ..saved("gone:owner", Anchor::Placed(chest(2)), &[4])
            },
        ],
        "a new chest built where chest 2 stood never takes its value"
    );
}

#[test]
fn a_value_set_before_the_first_step_is_found_after_the_next_load() {
    kinds();
    let bytes = file::write([saved("a:owner", Anchor::Placed(chest(1)), &[5])]);
    let (pending, unopened) = read_file(&bytes).unwrap();
    let mut world = server(pending, unopened);
    // A mod sets a value on the chest before the world's first step.
    world.values.insert(
        (1, 1027),
        Record {
            item: 9,
            anchor: Some(Anchor::Placed(chest(1))),
            value: Box::new([6]),
        },
    );
    let mut index = Index::default();
    index.add(chest(1), 1027);
    first_step(&mut world, &index);
    assert_eq!(&*world.values[&(1, 1027)].value, &[6]);
    // Restart: the chest is 1031 now.
    let (pending, unopened) = read_file(&write_file(&world)).unwrap();
    let mut world = server(pending, unopened);
    let mut index = Index::default();
    index.add(chest(1), 1031);
    first_step(&mut world, &index);
    assert_eq!(&*world.values[&(1, 1031)].value, &[6], "the live value");
    assert_eq!(world.pending.len(), 1, "the replaced one stays aside");
    assert!(world.pending[0].aside);
}

#[test]
fn a_save_keeps_values_by_anchor_and_records_aside_as_they_are() {
    kinds();
    // A file of the version that kept entity ids: its record is aside
    // under the old id, never attached.
    let mut old = file::MAGIC.to_vec();
    old.extend_from_slice(&[1, 7]);
    old.extend_from_slice(b"a:owner");
    old.extend_from_slice(&1043u32.to_le_bytes());
    old.extend_from_slice(&9u32.to_le_bytes());
    old.extend_from_slice(&[1, 0, 5]);
    let (pending, unopened) = read_file(&old).unwrap();
    assert_eq!(pending[0].anchor, Anchor::Session(1043));
    assert!(pending[0].aside);
    let mut kept = server(pending, unopened);
    kept.values.insert(
        (1, 1027),
        Record {
            item: 9,
            anchor: Some(Anchor::Placed(chest(1))),
            value: Box::new([6]),
        },
    );
    kept.values.insert(
        (1, 1030),
        Record {
            item: 9,
            anchor: None,
            value: Box::new([8]),
        },
    );
    let written = write_file(&kept);
    let records = file::read(&written).unwrap();
    let anchors: Vec<(Anchor, &[u8], bool)> = records
        .iter()
        .map(|r| (r.anchor, r.value, r.aside))
        .collect();
    assert_eq!(
        anchors,
        [
            (Anchor::Placed(chest(1)), &[6u8][..], false),
            (Anchor::Session(1030), &[8][..], false),
            (Anchor::Session(1043), &[5][..], true),
        ]
    );
    assert_eq!(read_file(b"EWID\x09"), None, "a newer file is not read");
}

#[test]
fn records_read_back_as_written_and_a_cut_one_ends_the_list() {
    let mut bytes = Vec::new();
    encode(&mut bytes, "a:charges", 900, 7, &[3]);
    encode(&mut bytes, "b:name", 901, 8, b"Oswald");
    encode(&mut bytes, "a:charges", 902, 7, &[]);
    let records = decode(&bytes);
    assert_eq!(
        records,
        [
            ("a:charges", 900, 7, &[3u8][..]),
            ("b:name", 901, 8, &b"Oswald"[..]),
            ("a:charges", 902, 7, &[][..]),
        ]
    );
    assert_eq!(decode(&bytes[..bytes.len() - 3]).len(), 2);
    assert!(decode(&[200, 1, 2]).is_empty());
}

#[test]
fn only_a_missing_world_file_starts_the_world_empty() {
    let dir = std::env::temp_dir().join(format!("enshwick-item-read-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let Ok((pending, unopened)) = read_world(&dir.join(FILE)) else {
        panic!("a missing file is an empty world");
    };
    assert!(pending.is_empty() && unopened.is_empty());
    // A folder in the file's place reads as an error, as a locked file does.
    std::fs::create_dir(dir.join(FILE)).unwrap();
    assert!(read_world(&dir.join(FILE)).is_err());
    let newer = dir.join("newer.bin");
    std::fs::write(&newer, b"EWID\x09").unwrap();
    assert!(read_world(&newer).is_err());
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_write_marks_the_same_world_changed_again() {
    let mut kept = Some(server(Vec::new(), Vec::new()));
    unwritten(&mut kept, "other");
    assert!(!kept.as_ref().unwrap().changed, "another world's write");
    unwritten(&mut kept, "w");
    assert!(kept.as_ref().unwrap().changed, "retried at the next save");
    let mut none = None;
    unwritten(&mut none, "w");
    assert!(none.is_none());
}

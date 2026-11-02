//! Tests of kind ids, destroy requests, the client's cap and the world file's records.

use super::attach::attach;
use super::carried::*;
use super::world_file::{read_file, read_world, unwritten, write_file};
use super::*;
use crate::item_records::anchor::{CharacterId, Index};
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
/// (0), an item kind clients get, `a:owner` (1), an entity kind, and the
/// item stats kind (2).
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
    let _stats = KINDS.add(Kind {
        id: enshwick::stats::ITEM_KIND.into(),
        scope: Scope::Item,
        sent: true,
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
        joining: Vec::new(),
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
    assert_eq!(keep_carried(&mut world, Entity(ANN), held), (0, 2));
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
fn a_leaving_character_s_items_wait_for_its_join_and_come_back_in_their_slots() {
    const ANN: u32 = 7;
    const ANN_GUID: CharacterId = CharacterId([0xa1; 16]);
    kinds();
    let mut world = server(Vec::new(), Vec::new());
    let value = |item, byte| Record {
        item,
        anchor: None,
        value: Box::new([byte]),
    };
    // Two items in Ann's backpack slots 1 and 2, and one whose
    // character GUID does not read.
    world.values.insert((0, 2001), value(77, 1));
    world.values.insert((0, 2002), value(78, 2));
    world.values.insert((0, 2003), value(79, 3));
    let left = |pide| {
        let (anchor, item) = match pide {
            2001 => (Some((0, 1)), 77),
            2002 => (Some((0, 2)), 78),
            2003 => (None, 79),
            _ => return None,
        };
        Some(game::Held {
            anchor: anchor.map(|(link, slot)| Anchor::Carried {
                character: ANN_GUID,
                link,
                slot,
            }),
            item,
            root: ANN,
        })
    };
    assert_eq!(keep_carried(&mut world, Entity(ANN), left), (2, 1));
    assert!(world.values.is_empty());
    assert!(world.changed);
    world.changed = false;
    // Rejoined: new item entities, slot 2 now holds another item.
    let joined = |link, slot| match (link, slot) {
        (0, 1) => Some((3001, 77)),
        (0, 2) => Some((3002, 90)),
        _ => None,
    };
    let back = take_back_carried(&mut world, ANN_GUID, joined);
    assert_eq!(back.found, [(0, 3001)]);
    assert_eq!(
        (back.replaced, back.waiting),
        (0, 1),
        "the slot holding another item"
    );
    let value = &world.values[&(0, 3001)];
    assert_eq!((value.item, &value.value[..]), (77, &[1][..]));
    let waiting: Vec<bool> = world.pending.iter().map(|r| r.aside).collect();
    assert_eq!(
        waiting.iter().filter(|&&aside| !aside).count(),
        1,
        "the changed slot waits on"
    );
    assert_eq!(world.pending.len(), 2);
    assert!(world.changed);
    // A later look, once the slot holds the item, takes the rest back.
    let filled = |link, slot| match (link, slot) {
        (0, 2) => Some((3002, 78)),
        _ => None,
    };
    let back = take_back_carried(&mut world, ANN_GUID, filled);
    assert_eq!((back.found, back.waiting), (vec![(0, 3002)], 0));
}

#[test]
fn a_second_leave_keeps_one_waiting_record_per_slot_with_the_newer_value() {
    const ANN: u32 = 7;
    const ANN_GUID: CharacterId = CharacterId([0xa1; 16]);
    kinds();
    let mut world = server(Vec::new(), Vec::new());
    let in_slot = |pide| {
        (pide == 2001 || pide == 3001).then_some(game::Held {
            anchor: Some(Anchor::Carried {
                character: ANN_GUID,
                link: 0,
                slot: 1,
            }),
            item: 77,
            root: ANN,
        })
    };
    let value = |byte| Record {
        item: 77,
        anchor: None,
        value: Box::new([byte]),
    };
    world.values.insert((0, 2001), value(1));
    assert_eq!(keep_carried(&mut world, Entity(ANN), in_slot), (1, 0));
    // Rejoined, nothing taken back in time, a new value set, left again.
    world.values.insert((0, 3001), value(2));
    assert_eq!(keep_carried(&mut world, Entity(ANN), in_slot), (1, 0));
    let pending: Vec<(u8, bool)> = world
        .pending
        .iter()
        .map(|r| (r.value[0], r.aside))
        .collect();
    assert_eq!(
        pending,
        [(1, true), (2, false)],
        "the older one kept, aside"
    );
    let back = take_back_carried(&mut world, ANN_GUID, |_, _| Some((4001, 77)));
    assert_eq!(back.found, [(0, 4001)]);
    assert_eq!(&*world.values[&(0, 4001)].value, &[2]);
}

#[test]
fn a_value_set_on_a_slot_an_older_record_waits_on_sets_that_record_aside() {
    const ANN_GUID: CharacterId = CharacterId([0xa1; 16]);
    let slot = Anchor::Carried {
        character: ANN_GUID,
        link: 0,
        slot: 1,
    };
    let mut pending = vec![Pending {
        kind: 0,
        anchor: slot,
        check: 77,
        value: Box::new([1]),
        aside: false,
    }];
    assert_eq!(
        set_aside_older(&mut pending, [(1, slot)].into_iter()),
        0,
        "another kind"
    );
    assert_eq!(set_aside_older(&mut pending, [(0, slot)].into_iter()), 1);
    assert!(pending[0].aside);
}

#[test]
fn a_slow_join_counts_its_looks_from_when_its_inventories_fill() {
    const ANN: Entity = Entity(7);
    const ANN_GUID: CharacterId = CharacterId([0xa1; 16]);
    kinds();
    let mut world = server(
        vec![Pending {
            kind: 0,
            anchor: Anchor::Carried {
                character: ANN_GUID,
                link: 0,
                slot: 1,
            },
            check: 77,
            value: Box::new([1]),
            aside: false,
        }],
        Vec::new(),
    );
    let characters = [(ANN, ANN_GUID)];
    let mut joining = Joining::new(ANN);
    let mut found = Vec::new();
    // Not filled for longer than the looks a filled join gets.
    for _ in 0..JOIN_LOOKS * 2 {
        let look = look(
            &mut world,
            &characters,
            &mut joining,
            &mut found,
            |_| false,
            |_, _, _| None,
        );
        assert!(matches!(look, Look::Again));
    }
    // Filled, the slot still empty, then holding the item.
    let empty = look(
        &mut world,
        &characters,
        &mut joining,
        &mut found,
        |_| true,
        |_, _, _| None,
    );
    assert!(matches!(empty, Look::Again));
    let filled = look(
        &mut world,
        &characters,
        &mut joining,
        &mut found,
        |_| true,
        |_, _, _| Some((3001, 77)),
    );
    assert!(matches!(filled, Look::Done));
    assert_eq!(found, [(0, 3001)]);
}

#[test]
fn a_join_that_never_fills_stops_at_the_cap_and_its_records_keep_waiting() {
    const ANN: Entity = Entity(7);
    const ANN_GUID: CharacterId = CharacterId([0xa1; 16]);
    kinds();
    let record = Pending {
        kind: 0,
        anchor: Anchor::Carried {
            character: ANN_GUID,
            link: 0,
            slot: 1,
        },
        check: 77,
        value: Box::new([1]),
        aside: false,
    };
    let mut world = server(vec![record.clone()], Vec::new());
    let mut joining = Joining::new(ANN);
    let mut found = Vec::new();
    let mut stopped = None;
    for at in 1..=JOIN_LOOKS_CAP {
        if let Look::Stop(why) = look(
            &mut world,
            &[(ANN, ANN_GUID)],
            &mut joining,
            &mut found,
            |_| false,
            |_, _, _| None,
        ) {
            stopped = Some((at, why));
            break;
        }
    }
    assert_eq!(stopped.map(|(at, _)| at), Some(JOIN_LOOKS_CAP));
    assert!(stopped.is_some_and(|(_, why)| why.contains("never")));
    assert_eq!(world.pending, [record], "nothing went aside");
}

#[test]
fn a_leaving_character_s_records_whose_slot_lost_their_item_go_aside_and_others_stay() {
    let record = |character, slot, aside| Pending {
        kind: 0,
        anchor: Anchor::Carried {
            character: CharacterId([character; 16]),
            link: 0,
            slot,
        },
        check: 77,
        value: Box::new([1]),
        aside,
    };
    let mut pending = vec![
        record(0xa1, 1, false),
        record(0xa1, 2, false),
        record(0xa1, 3, true),
        record(0xb2, 1, false),
    ];
    // Slot 1 is empty now; slot 2 holds an item of the record's id.
    let item_in = |_, slot| (slot == 2).then_some(77);
    assert_eq!(
        set_aside_waiting(&mut pending, CharacterId([0xa1; 16]), item_in),
        1
    );
    let aside: Vec<bool> = pending.iter().map(|r| r.aside).collect();
    assert_eq!(aside, [true, false, true, false]);
}

#[test]
fn records_batch_into_messages_no_longer_than_the_limit_in_their_order() {
    let mut messages = Vec::new();
    let mut whole = Vec::new();
    for pide in 0..100 {
        push_encoded(&mut messages, "a:charges", pide, 7, &[3; 20]);
        encode(&mut whole, "a:charges", pide, 7, &[3; 20]);
    }
    assert!(messages.len() > 1);
    assert!(
        messages
            .iter()
            .all(|m| !m.is_empty() && m.len() <= MESSAGE_BYTES)
    );
    assert_eq!(messages.concat(), whole);
}

#[test]
fn a_taken_back_item_stats_value_makes_the_wearers_look_again() {
    kinds();
    let stats = kind_of(enshwick::stats::ITEM_KIND).unwrap();
    assert!(touches_item_stats(&[(0, 3001), (stats, 3002)]));
    assert!(!touches_item_stats(&[(0, 3001)]));
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

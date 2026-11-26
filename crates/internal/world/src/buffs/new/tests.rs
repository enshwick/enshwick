use super::*;

/// The offsets of reflection 1269579.
const LAYOUT: BuffLayout = BuffLayout {
    buffs: 24,
    entries: 0,
    keys: 8,
    values: 16,
    bucket: 8,
    bucket_start: 0,
    bucket_count: 4,
    size: 184,
    id: 0,
    slot: 4,
    life_time: 8,
    name: 16,
    description: 32,
    hide_in_ui: 49,
    icon: 52,
    hide_duration_in_ui: 72,
    is_saved: 74,
    impact: 76,
    impact_size: 16,
    on_timeout: 136,
    on_timeout_size: 16,
    on_timeout_is_set: 152,
    relative: [96, 104, 112, 120, 128, 156, 164, 172],
};
const BUCKETS: u32 = 4;
/// Three vanilla types, the first and the third in one bucket.
const VANILLA: [u32; 3] = [0x10, 0x23, 0x30];
const FOOD: BuffTypeId = BuffTypeId(0x10);
const NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
const ICON: Guid = Guid::parse("02778393-2031-44cd-b7cd-3bc215acd7af");
/// The food type's one tooltip line.
const LINE: [u8; 4] = [7, 7, 7, 7];

/// A collection of the [`VANILLA`] types, in reverse order in the buff types
/// so a value is not its key's place; the food type has a tooltip line,
/// an impact program and a follow-up.
fn collection() -> Vec<u8> {
    let mut blob = Blob::new(&[0u8; 32]);
    let mut buffs = Vec::new();
    for &id in VANILLA.iter().rev() {
        let mut buff = vec![0u8; LAYOUT.size];
        buff[..4].copy_from_slice(&id.to_le_bytes());
        buff[LAYOUT.life_time..LAYOUT.life_time + 8]
            .copy_from_slice(&600_000_000_000i64.to_le_bytes());
        buff[LAYOUT.is_saved] = 1;
        if id == FOOD.0 {
            buff[LAYOUT.impact] = 0xaa;
            buff[LAYOUT.on_timeout] = 0xbb;
            buff[LAYOUT.on_timeout_is_set] = 1;
        }
        buffs.extend(buff);
    }
    let at = blob.append(&buffs, 8);
    assert!(blob.set_array(LAYOUT.buffs, at, 3));
    let food = at + 2 * LAYOUT.size;
    let line = blob.append(&LINE, 4);
    assert!(blob.set_array(food + LAYOUT.relative[2], line, 1));
    let entries = blob.append(&[0; BUCKETS as usize * 8], 4);
    assert!(blob.set_array(LAYOUT.entries, entries, BUCKETS));
    for field in [LAYOUT.keys, LAYOUT.values] {
        let at = blob.append(&[0; 12], 4);
        assert!(blob.set_array(field, at, 3));
    }
    let mut pairs = vec![(0x10, 2), (0x23, 1), (0x30, 0)];
    assert_eq!(by_bucket(&mut pairs, BUCKETS), Some(()));
    assert_eq!(write_map(&LAYOUT, &mut blob, &pairs, 0), Some(()));
    blob.into_bytes()
}

/// The id of the buff type at `at`.
fn id_at(bytes: &[u8], at: usize) -> Option<u32> {
    Blob::reading(bytes).read_u32(at + LAYOUT.id)
}

/// Where the game's `buffs.type_by_id` finds `id`: its bucket's keys, then
/// the buff type the value indexes.
fn lookup(bytes: &[u8], id: u32) -> Option<usize> {
    let blob = Blob::reading(bytes);
    let (entries, buckets) = blob.array(LAYOUT.entries)?;
    let (keys, _) = blob.array(LAYOUT.keys)?;
    let (values, _) = blob.array(LAYOUT.values)?;
    let (buffs, _) = blob.array(LAYOUT.buffs)?;
    let bucket = entries + (id & (buckets - 1)) as usize * LAYOUT.bucket;
    let first = blob.read_u32(bucket + LAYOUT.bucket_start)? as usize;
    let len = blob.read_u32(bucket + LAYOUT.bucket_count)? as usize;
    let i = (first..first + len).find(|&i| blob.read_u32(keys + i * 4) == Some(id))?;
    Some(buffs + blob.read_u32(values + i * 4)? as usize * LAYOUT.size)
}

fn edit(id: u32) -> Edit {
    Edit {
        id: BuffTypeId(id),
        like: FOOD,
        name: None,
        description: None,
        icon: None,
        life_time: None,
        slot: None,
        hide_in_ui: None,
        hide_duration_in_ui: None,
        is_saved: None,
        plain: false,
    }
}

/// `base` with `types`, as the loader serves it: the base as it is when it
/// does not read. A build reserves its final length once, up front.
fn add(base: &[u8], types: &[Edit]) -> Vec<u8> {
    let mut out = Vec::new();
    match build(&LAYOUT, base, types, &mut out) {
        Some(_) => {
            assert_eq!(
                out.capacity(),
                out.len(),
                "the build grew past what it reserved"
            );
            out
        }
        None => base.to_vec(),
    }
}

#[test]
fn the_games_lookup_finds_the_new_type_with_its_changes_and_every_vanilla_one() {
    let lucky = Edit {
        name: Some(NAME),
        icon: Some(ICON),
        life_time: Some(300_000_000_000),
        slot: Some(1),
        is_saved: Some(false),
        ..edit(0x47)
    };
    let built = add(&collection(), &[lucky]);
    let at = lookup(&built, 0x47).expect("the new type is found");
    assert_eq!(id_at(&built, at), Some(0x47));
    assert_eq!(&built[at + LAYOUT.name..at + LAYOUT.name + 16], &NAME.0);
    assert_eq!(&built[at + LAYOUT.icon..at + LAYOUT.icon + 16], &ICON.0);
    assert_eq!(built[at + LAYOUT.slot], 1);
    assert_eq!(built[at + LAYOUT.is_saved], 0);
    assert_eq!(
        i64::from_le_bytes(
            built[at + LAYOUT.life_time..at + LAYOUT.life_time + 8]
                .try_into()
                .unwrap()
        ),
        300_000_000_000
    );
    for id in VANILLA {
        let at = lookup(&built, id).expect("a vanilla type stays found");
        assert_eq!(id_at(&built, at), Some(id));
    }
}

#[test]
fn one_patch_adds_every_type_of_a_mod_in_one_build() {
    let types = [
        edit(0x47),
        edit(0x53),
        Edit {
            like: BuffTypeId(0x23),
            ..edit(0x64)
        },
    ];
    let built = add(&collection(), &types);
    let blob = Blob::reading(&built);
    assert_eq!(blob.array(LAYOUT.buffs).map(|(_, n)| n), Some(6));
    assert_eq!(blob.array(LAYOUT.keys).map(|(_, n)| n), Some(6));
    assert_eq!(blob.array(LAYOUT.values).map(|(_, n)| n), Some(6));
    for id in [0x10, 0x23, 0x30, 0x47, 0x53, 0x64] {
        assert_eq!(
            lookup(&built, id).and_then(|at| id_at(&built, at)),
            Some(id),
            "type {id:#x}"
        );
    }
}

#[test]
fn the_copy_reads_the_vanilla_types_tooltip_lines_where_they_are() {
    let built = add(&collection(), &[edit(0x47)]);
    let blob = Blob::reading(&built);
    for id in [FOOD.0, 0x47] {
        let at = lookup(&built, id).unwrap();
        let (line, count) = blob.array(at + LAYOUT.relative[2]).unwrap();
        assert_eq!(
            (count, &built[line..line + 4]),
            (1, &LINE[..]),
            "type {id:#x}"
        );
    }
    let copy = lookup(&built, 0x47).unwrap();
    assert_eq!(
        built[copy + LAYOUT.impact],
        0xaa,
        "a copy that is not plain keeps the impact"
    );
}

#[test]
fn a_plain_copy_drops_what_the_vanilla_type_does() {
    let built = add(
        &collection(),
        &[Edit {
            plain: true,
            ..edit(0x47)
        }],
    );
    let at = lookup(&built, 0x47).unwrap();
    assert_eq!(
        &built[at + LAYOUT.impact..at + LAYOUT.impact + 16],
        &[0; 16]
    );
    assert_eq!(
        &built[at + LAYOUT.on_timeout..at + LAYOUT.on_timeout + 17],
        &[0; 17]
    );
    for header in LAYOUT.relative {
        assert_eq!(
            &built[at + header..at + header + 8],
            &[0; 8],
            "header at {header}"
        );
    }
    let food = lookup(&built, FOOD.0).unwrap();
    assert_eq!(
        built[food + LAYOUT.impact],
        0xaa,
        "the vanilla type keeps its own"
    );
}

#[test]
fn two_mods_add_types_one_after_the_other() {
    let once = add(&collection(), &[edit(0x47)]);
    let twice = add(
        &once,
        &[Edit {
            like: BuffTypeId(0x23),
            ..edit(0x53)
        }],
    );
    for id in [0x10, 0x23, 0x30, 0x47, 0x53] {
        assert_eq!(
            lookup(&twice, id).and_then(|at| id_at(&twice, at)),
            Some(id)
        );
    }
}

#[test]
fn a_type_an_earlier_patch_made_impossible_is_left_out_and_the_rest_served() {
    let base = add(&collection(), &[edit(0x47)]);
    let types = [
        edit(0x47),
        Edit {
            like: BuffTypeId(0x99),
            ..edit(0x53)
        },
        edit(0x64),
    ];
    let built = add(&base, &types);
    let blob = Blob::reading(&built);
    assert_eq!(blob.array(LAYOUT.buffs).map(|(_, n)| n), Some(5));
    assert!(lookup(&built, 0x53).is_none());
    assert_eq!(
        lookup(&built, 0x64).and_then(|at| id_at(&built, at)),
        Some(0x64)
    );
}

#[test]
fn a_build_that_can_add_nothing_serves_the_base_as_it_is() {
    let base = collection();
    assert_eq!(add(&base, &[edit(0x23)]), base, "a taken id");
    assert_eq!(add(&base, &[]), base, "no types");
}

#[test]
fn a_map_whose_buckets_overrun_its_keys_serves_the_base_as_it_is() {
    let mut base = Blob::new(&collection());
    let (entries, _) = base.array(LAYOUT.entries).unwrap();
    assert!(base.write(entries + LAYOUT.bucket_count, &9u32.to_le_bytes()));
    let base = base.into_bytes();
    assert_eq!(add(&base, &[edit(0x47)]), base);
}

#[test]
fn the_check_at_init_refuses_what_the_build_could_not_add() {
    let base = collection();
    let refused = |edit: Edit, added: &[Edit]| check(&LAYOUT, &base, &edit, added);
    assert_eq!(refused(edit(0x47), &[]), Ok(()));
    assert_eq!(
        refused(
            Edit {
                like: BuffTypeId(0x99),
                ..edit(0x47)
            },
            &[]
        ),
        Err(Refusal::NotVanilla(BuffTypeId(0x99)))
    );
    assert_eq!(
        refused(edit(0x23), &[]),
        Err(Refusal::Taken),
        "a vanilla id"
    );
    assert_eq!(
        refused(edit(0x47), &[edit(0x53), edit(0x47)]),
        Err(Refusal::Taken),
        "an id a mod before took"
    );
    let mut broken = Blob::new(&base);
    let (entries, _) = broken.array(LAYOUT.entries).unwrap();
    assert!(broken.write(entries + LAYOUT.bucket_count, &9u32.to_le_bytes()));
    assert_eq!(
        check(&LAYOUT, broken.bytes(), &edit(0x47), &[]),
        Err(Refusal::Unread)
    );
}

#[test]
fn two_types_of_one_id_in_one_build_serve_the_first() {
    let types = [
        edit(0x47),
        Edit {
            like: BuffTypeId(0x23),
            ..edit(0x47)
        },
    ];
    let mut out = Vec::new();
    let left = build(&LAYOUT, &collection(), &types, &mut out);
    assert_eq!(left, Some(vec![BuffTypeId(0x47)]));
    assert_eq!(
        Blob::reading(&out).array(LAYOUT.buffs).map(|(_, n)| n),
        Some(4)
    );
}

#[test]
fn a_request_reads_as_the_edit_it_names() {
    let name = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
    let mut request = enshwick_abi::headed!(BuffTypeAdd {
        guid: Guid([3; 16]),
        name,
        description: Guid([9; 16]),
        icon: ICON,
        life_time_ns: 5,
        like: FOOD,
        changes: BuffTypeAdd::NAME
            | BuffTypeAdd::ICON
            | BuffTypeAdd::LIFE_TIME
            | BuffTypeAdd::SET_SAVED
            | BuffTypeAdd::SET_HIDE_IN_UI,
        slot: 4,
        flags: BuffTypeAdd::HIDE_IN_UI | BuffTypeAdd::PLAIN,
    });
    let read = Edit::of(&request).expect("a request it takes");
    assert_eq!(
        read,
        Edit {
            id: BuffTypeId::from_guid(Guid([3; 16])),
            name: Some(name),
            icon: Some(ICON),
            life_time: Some(5),
            hide_in_ui: Some(true),
            is_saved: Some(false),
            plain: true,
            ..edit(0)
        }
    );
    assert!(read.icon().is_some());
    assert_eq!(read.without_icon().icon(), None);
    request.changes |= 1 << 8;
    assert_eq!(Edit::of(&request), None, "a change it does not know");
    request.changes = 0;
    request.flags = 1 << 4;
    assert_eq!(Edit::of(&request), None, "a flag it does not know");
    request.flags = 0;
    request.life_time_ns = -1;
    assert_eq!(Edit::of(&request), None, "a negative life time");
}

#[test]
fn a_new_buff_asks_for_what_it_sets_and_nothing_else() {
    let lucky = NewBuff::new(Id::new("you.lucky:lucky"), FOOD)
        .name(NAME)
        .default_life_time(Duration::from_secs(2))
        .slot(BuffSlotType::Buff)
        .is_saved(false)
        .plain();
    let request = lucky.request().unwrap();
    assert_eq!(
        request.changes,
        BuffTypeAdd::NAME | BuffTypeAdd::LIFE_TIME | BuffTypeAdd::SLOT | BuffTypeAdd::SET_SAVED
    );
    assert_eq!(request.flags, BuffTypeAdd::PLAIN, "saved is false");
    assert_eq!(request.life_time_ns, 2_000_000_000);
    assert_eq!(request.guid, Id::new("you.lucky:lucky").guid());
    let read = Edit::of(&request).unwrap();
    assert_eq!(read.id, BuffTypeId::from_guid(request.guid));
    assert_eq!(read.is_saved, Some(false));
    assert_eq!(
        NewBuff::new(Id::new("you.lucky:odd"), FOOD)
            .slot(BuffSlotType::Other(300))
            .request(),
        Err("the slot is past a byte")
    );
}

#[test]
fn slots_go_back_to_the_games_values() {
    for slot in [
        BuffSlotType::Food,
        BuffSlotType::Buff,
        BuffSlotType::Debuff,
        BuffSlotType::Rested,
        BuffSlotType::TerrainDebuff,
        BuffSlotType::Other(9),
    ] {
        assert_eq!(
            slot.to_slot()
                .map(|s| BuffSlotType::from_slot(u16::from(s))),
            Some(slot)
        );
    }
    assert_eq!(BuffSlotType::Other(300).to_slot(), None);
}

use enshwick_core::abi::BuffTypeId;
use enshwick_core::buffs::BuffLayout;
use enshwick_core::content::{Blob, Guid};

use super::*;

/// A vanilla food type of client 26430625.
const FOOD: BuffTypeId = BuffTypeId(0xf9b3_64f6);
const RESTED: BuffTypeId = BuffTypeId(0xcdbe_3e80);
const ICON: Guid = Guid([5; 16]);

/// The offsets of client reflection 1269579.
fn layout() -> BuffLayout {
    {
        let mut value = BuffLayout::default();
        value.buffs = 24;
        value.entries = 0;
        value.keys = 8;
        value.values = 16;
        value.bucket = 8;
        value.bucket_start = 0;
        value.bucket_count = 4;
        value.size = 184;
        value.id = 0;
        value.slot = 4;
        value.life_time = 8;
        value.name = 16;
        value.description = 32;
        value.hide_in_ui = 49;
        value.icon = 52;
        value.hide_duration_in_ui = 72;
        value.is_saved = 74;
        value.impact = 76;
        value.impact_size = 16;
        value.on_timeout = 136;
        value.on_timeout_size = 16;
        value.on_timeout_is_set = 152;
        value.relative = [96, 104, 112, 120, 128, 156, 164, 172];
        value
    }
}

/// A vanilla collection of `ids`, every field zero but the id, all in one
/// bucket.
fn vanilla(layout: &BuffLayout, ids: &[BuffTypeId]) -> Vec<u8> {
    let count = u32::try_from(ids.len()).unwrap();
    let mut blob = Blob::new(&[0u8; 32]);
    let mut buffs = vec![0; layout.size * ids.len()];
    let (mut keys, mut values) = (Vec::new(), Vec::new());
    for (i, (id, buff)) in ids
        .iter()
        .zip(buffs.chunks_exact_mut(layout.size))
        .enumerate()
    {
        buff[layout.id..layout.id + 4].copy_from_slice(&id.0.to_le_bytes());
        keys.extend(id.0.to_le_bytes());
        values.extend(u32::try_from(i).unwrap().to_le_bytes());
    }
    let mut entry = vec![0; layout.bucket];
    entry[layout.bucket_count..layout.bucket_count + 4].copy_from_slice(&count.to_le_bytes());
    for (field, bytes, align, count) in [
        (layout.buffs, buffs, 8, count),
        (layout.entries, entry, 4, 1),
        (layout.keys, keys, 4, count),
        (layout.values, values, 4, count),
    ] {
        let at = blob.append(&bytes, align);
        assert!(blob.set_array(field, at, count));
    }
    blob.into_bytes()
}

fn request(n: u8, like: BuffTypeId) -> BuffTypeAdd {
    enshwick_abi::headed!(BuffTypeAdd {
        guid: Guid([n; 16]),
        name: Guid::NIL,
        description: Guid::NIL,
        icon: ICON,
        life_time_ns: 0,
        like,
        changes: BuffTypeAdd::ICON,
        slot: 0,
        flags: 0,
    })
}

fn mod_index(n: usize) -> ModIndex {
    ModIndex::new(n)
}

/// Places `requests` of mods 1, 2, … in order, as the loader adds them.
fn place_all(requests: &[BuffTypeAdd]) -> (Vec<Added>, Vec<Result<BuffTypeId, BuffTypeError>>) {
    let layout = layout();
    let base = vanilla(&layout, &[FOOD, RESTED]);
    let mut added = Vec::new();
    let answers = requests
        .iter()
        .enumerate()
        .map(|(n, request)| {
            let edit = place(&layout, &base, &serving(&added, |_| true), request)?;
            added.push(Added {
                owner: mod_index(n + 1),
                edit,
            });
            Ok(edit.id())
        })
        .collect();
    (added, answers)
}

#[test]
fn a_second_mod_taking_an_id_a_mod_before_took_is_refused_at_its_init() {
    let (added, answers) = place_all(&[request(1, FOOD), request(1, RESTED), request(2, RESTED)]);
    assert_eq!(answers[0], Ok(BuffTypeId::from_guid(Guid([1; 16]))));
    assert!(matches!(
        answers[1],
        Err(BuffTypeError::Refused(Refusal::Taken))
    ));
    assert!(answers[2].is_ok());
    assert_eq!(added.len(), 2);
}

#[test]
fn a_request_the_game_cannot_take_is_refused_with_its_reason() {
    let mut odd = request(3, FOOD);
    odd.changes |= 1 << 12;
    let (_, answers) = place_all(&[
        request(1, BuffTypeId(0x99)),
        request(2, BuffTypeId::from_guid(Guid([2; 16]))),
        odd,
    ]);
    assert!(matches!(
        answers[0],
        Err(BuffTypeError::Refused(Refusal::NotVanilla(_)))
    ));
    assert!(
        matches!(
            answers[1],
            Err(BuffTypeError::Refused(Refusal::NotVanilla(_)))
        ),
        "a mod's own type is no vanilla one to copy"
    );
    assert_eq!(answers[2], Err(BuffTypeError::BadRequest));
}

#[test]
fn a_mod_turned_off_serves_no_types_and_the_rest_keep_their_order() {
    let (added, _) = place_all(&[request(1, FOOD), request(2, FOOD), request(3, RESTED)]);
    let served = serving(&added, |owner| owner != mod_index(2));
    let ids: Vec<BuffTypeId> = served.iter().map(Edit::id).collect();
    assert_eq!(
        ids,
        [Guid([1; 16]), Guid([3; 16])].map(BuffTypeId::from_guid)
    );
}

#[test]
fn icons_are_named_only_once_the_client_shows_them_and_the_length_holds() {
    let layout = layout();
    let base = vanilla(&layout, &[FOOD]);
    let (added, _) = place_all(&[request(1, FOOD)]);
    let types = serving(&added, |_| true);
    let build = |icons: bool| {
        let mut out = Vec::new();
        let left = enshwick_core::__private::rules::buff_types::build(
            &layout,
            &base,
            &named(&types, |_| icons),
            &mut out,
        );
        assert_eq!(left, Some(Vec::new()));
        out
    };
    let (before, after) = (build(false), build(true));
    assert_eq!(
        before.len(),
        after.len(),
        "a rebuild writes over the game's copy"
    );
    let differ: Vec<usize> = (0..before.len())
        .filter(|&i| before[i] != after[i])
        .collect();
    assert_eq!(differ.len(), ICON.0.len(), "only the icon changes");
    let at = differ[0];
    assert_eq!(&after[at..at + 16], &ICON.0);
    assert_eq!(&before[at..at + 16], &[0; 16], "the vanilla icon before");
}

#[test]
fn an_icon_the_client_s_ui_does_not_know_is_never_named() {
    let (added, _) = place_all(&[request(1, FOOD)]);
    let types = serving(&added, |_| true);
    assert_eq!(named(&types, |guid| guid == ICON)[0].icon(), Some(ICON));
    assert_eq!(named(&types, |guid| guid != ICON)[0].icon(), None);
}

#[test]
fn an_id_a_mod_turned_off_took_is_free_for_a_later_mod() {
    let layout = layout();
    let base = vanilla(&layout, &[FOOD]);
    let (added, _) = place_all(&[request(1, FOOD)]);
    let taken = request(1, FOOD);
    let on = serving(&added, |owner| owner != mod_index(1));
    assert!(place(&layout, &base, &on, &taken).is_ok());
    let all = serving(&added, |_| true);
    assert_eq!(
        place(&layout, &base, &all, &taken),
        Err(BuffTypeError::Refused(Refusal::Taken))
    );
}

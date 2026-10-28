//! Keys that find a record's entity again after a load. The game numbers
//! entities anew at every load, in save order, so an entity id names its
//! entity for one session only; the save keeps no identity of its own
//! (server 399). What it keeps is the template and the position of every
//! placed thing, and an item's place in its owner's inventory.

use rustc_hash::FxHashMap;

/// The low bits of each 32.32 fixed-point axis the save clears: it keeps a
/// position floored to 2^-14 m, and a loaded position is kept unchanged
/// (server 399, proven for positive axes). Orientation is re-encoded by a
/// codec not known, so it is no part of a key.
const SAVED_GRAIN: i64 = (1 << 18) - 1;

/// A placed thing: its template GUID and where the save keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Placed {
    template: [u8; 16],
    position: [i64; 3],
}

impl Placed {
    /// The key of a thing of `template` at `position`, 32.32 fixed-point
    /// metres, floored as the save keeps it, so a thing spawned or moved
    /// this session keys as it will after the next load.
    pub(crate) fn new(template: [u8; 16], position: [i64; 3]) -> Self {
        Self {
            template,
            position: position.map(|axis| axis & !SAVED_GRAIN),
        }
    }

    pub(crate) const fn template(&self) -> [u8; 16] {
        self.template
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.template);
        for axis in self.position {
            out.extend_from_slice(&axis.to_le_bytes());
        }
    }

    fn decode(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let (template, mut rest) = bytes.split_first_chunk::<16>()?;
        let mut position = [0i64; 3];
        for axis in &mut position {
            let (bytes, after) = rest.split_first_chunk::<8>()?;
            *axis = i64::from_le_bytes(*bytes);
            rest = after;
        }
        Some((Self::new(*template, position), rest))
    }
}

/// What finds a record's entity after a load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Anchor {
    /// Only the entity id of the session that kept it, which a load gives to
    /// another entity: from a file that kept ids, or an entity that had no
    /// key when the world saved. Never matched; the record stays aside.
    Session(u32),
    /// A thing that stands in the world.
    Placed(Placed),
    /// An item in slot `slot` of the inventory linked at `link` to the
    /// placed thing `root`.
    Held { root: Placed, link: u8, slot: u8 },
}

const SESSION: u8 = 0;
const PLACED: u8 = 1;
const HELD: u8 = 2;

impl Anchor {
    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Session(id) => {
                out.push(SESSION);
                out.extend_from_slice(&id.to_le_bytes());
            }
            Self::Placed(placed) => {
                out.push(PLACED);
                placed.encode(out);
            }
            Self::Held { root, link, slot } => {
                out.push(HELD);
                root.encode(out);
                out.extend_from_slice(&[*link, *slot]);
            }
        }
    }

    /// The anchor `bytes` start with, and the bytes after it.
    pub(crate) fn decode(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let (&tag, rest) = bytes.split_first()?;
        match tag {
            SESSION => {
                let (id, rest) = rest.split_first_chunk::<4>()?;
                Some((Self::Session(u32::from_le_bytes(*id)), rest))
            }
            PLACED => Placed::decode(rest).map(|(p, rest)| (Self::Placed(p), rest)),
            HELD => {
                let (root, rest) = Placed::decode(rest)?;
                let (&[link, slot], rest) = rest.split_first_chunk::<2>()?;
                Some((Self::Held { root, link, slot }, rest))
            }
            _ => None,
        }
    }
}

/// What the world holds under one key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    One(u32),
    /// More than one entity: the key names none of them for certain.
    Many,
}

/// The placed things of the world by key.
#[derive(Debug, Default)]
pub(crate) struct Index(FxHashMap<Placed, Found>);

impl Index {
    pub(crate) fn add(&mut self, placed: Placed, entity: u32) {
        self.0
            .entry(placed)
            .and_modify(|found| {
                if *found != Found::One(entity) {
                    *found = Found::Many;
                }
            })
            .or_insert(Found::One(entity));
    }

    pub(crate) fn find(&self, placed: &Placed) -> Option<Found> {
        self.0.get(placed).copied()
    }
}

/// A record read from a file that has not found its entity: its kind, its
/// anchor, what the entity was (the template's id or the item it held), its
/// value, and whether it is aside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    pub(crate) kind: u32,
    pub(crate) anchor: Anchor,
    pub(crate) check: u32,
    pub(crate) value: Box<[u8]>,
    /// Never matched again: a record that once failed to name exactly one
    /// entity, whose place a later entity may take. Kept so no value is
    /// lost; a person may still give it back by hand.
    pub(crate) aside: bool,
}

/// What a pass over the pending records found.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    /// Records with their entity: kind, entity, record.
    pub(crate) found: Vec<(u32, u32, Pending)>,
    /// Records aside from now on, written back as they were.
    pub(crate) left: Vec<Pending>,
    /// Of `left`: places the world or the file holds more than once.
    pub(crate) ambiguous: usize,
    /// Of `left`: places no entity holds.
    pub(crate) missing: usize,
    /// Of `left`: records whose entity already had a value of the kind.
    pub(crate) replaced: usize,
    /// Of `left`: records aside before this pass, session ids among them.
    pub(crate) earlier: usize,
}

/// Finds each pending record's entity, once per load, when the world holds
/// every entity its save holds (a load creates them all before the
/// first step). `held(root, link, slot)` is the item entity in that slot and
/// the item it holds; `taken(kind, entity)` whether the entity already has a
/// value of the kind. A record attaches only to the one entity its place
/// names; every other record is aside from now on, since a later entity may
/// take its place.
pub(crate) fn resolve(
    pending: Vec<Pending>,
    index: &Index,
    held: impl Fn(u32, u8, u8) -> Option<(u32, u32)>,
    taken: impl Fn(u32, u32) -> bool,
) -> Outcome {
    let mut uses: FxHashMap<(u32, Anchor), usize> = FxHashMap::default();
    for record in pending.iter().filter(|r| !r.aside) {
        *uses.entry((record.kind, record.anchor)).or_default() += 1;
    }
    let mut outcome = Outcome::default();
    for mut record in pending {
        let target = match record.anchor {
            _ if record.aside => {
                outcome.earlier += 1;
                outcome.left.push(record);
                continue;
            }
            Anchor::Session(_) => {
                outcome.earlier += 1;
                record.aside = true;
                outcome.left.push(record);
                continue;
            }
            // The file names one place twice: neither record is certain.
            _ if uses[&(record.kind, record.anchor)] > 1 => Some(Found::Many),
            Anchor::Placed(placed) => index.find(&placed),
            Anchor::Held { root, link, slot } => match index.find(&root) {
                Some(Found::One(root)) => held(root, link, slot)
                    .filter(|&(_, item)| item == record.check)
                    .map(|(pide, _)| Found::One(pide)),
                other => other,
            },
        };
        let count = match target {
            Some(Found::One(entity)) if !taken(record.kind, entity) => {
                outcome.found.push((record.kind, entity, record));
                continue;
            }
            Some(Found::One(_)) => &mut outcome.replaced,
            Some(Found::Many) => &mut outcome.ambiguous,
            None => &mut outcome.missing,
        };
        *count += 1;
        record.aside = true;
        outcome.left.push(record);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHEST: [u8; 16] = [7; 16];
    const BED: [u8; 16] = [9; 16];

    /// A position `x` metres along the first axis, in 32.32 fixed point.
    fn at(x: i64) -> [i64; 3] {
        [x << 32, 900 << 32, 1400 << 32]
    }

    fn pending(kind: u32, anchor: Anchor, check: u32, value: u8) -> Pending {
        Pending {
            kind,
            anchor,
            check,
            value: Box::new([value]),
            aside: false,
        }
    }

    fn chest(x: i64) -> Placed {
        Placed::new(CHEST, at(x))
    }

    #[test]
    fn a_position_keys_as_the_save_keeps_it() {
        // Server 399: a spawned chest at x + 0x60000 loads at x + 0x40000;
        // + 0x7ffff loads at + 0x40000, + 0x20000 at + 0.
        let [x, y, z] = at(3);
        let spawned = |dx: i64| Placed::new(BED, [x + dx, y, z]);
        assert_eq!(spawned(0x60000), spawned(0x40000));
        assert_eq!(spawned(0x7ffff), spawned(0x40000));
        assert_eq!(spawned(0x20000), spawned(0));
        assert_ne!(spawned(0x40000), spawned(0), "one grain apart");
        assert_ne!(Placed::new(BED, at(3)), Placed::new(CHEST, at(3)));
    }

    #[test]
    fn anchors_read_back_as_written_and_a_cut_one_reads_as_none() {
        let anchors = [
            Anchor::Session(1043),
            Anchor::Placed(chest(5)),
            Anchor::Held {
                root: chest(6),
                link: 2,
                slot: 7,
            },
        ];
        let mut bytes = Vec::new();
        for anchor in &anchors {
            anchor.encode(&mut bytes);
        }
        let mut rest = bytes.as_slice();
        for anchor in anchors {
            let (read, after) = Anchor::decode(rest).unwrap();
            assert_eq!(read, anchor);
            rest = after;
        }
        assert!(rest.is_empty());
        let held = 1 + 16 + 24 + 2;
        let last = &bytes[bytes.len() - held..];
        assert!(Anchor::decode(last).is_some());
        assert_eq!(Anchor::decode(&last[..held - 1]), None);
        assert_eq!(Anchor::decode(&[9, 0, 0, 0, 0]), None);
    }

    #[test]
    fn records_find_their_entities_after_a_load_renumbers_them() {
        // Ten chests saved ahead of the older ones move every older
        // chest up by ten; the keys follow the chests, not the ids.
        let records: Vec<Pending> = (0..10)
            .map(|n| pending(0, Anchor::Placed(chest(n)), 1, u8::try_from(n).unwrap()))
            .collect();
        let mut index = Index::default();
        for n in 0..10 {
            index.add(chest(n), 1037 + u32::try_from(n).unwrap());
        }
        for n in 0..10 {
            index.add(chest(100 + n), 1027 + u32::try_from(n).unwrap());
        }
        let outcome = resolve(records, &index, |_, _, _| None, |_, _| false);
        assert!(outcome.left.is_empty());
        let found: Vec<(u32, u8)> = outcome
            .found
            .iter()
            .map(|(_, entity, record)| (*entity, record.value[0]))
            .collect();
        assert_eq!(
            found,
            (0..10)
                .map(|n| (1037 + n, u8::try_from(n).unwrap()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_key_two_entities_share_keeps_its_record_aside() {
        let mut index = Index::default();
        index.add(chest(1), 1027);
        index.add(chest(1), 1028);
        index.add(chest(1), 1027);
        let outcome = resolve(
            vec![pending(0, Anchor::Placed(chest(1)), 1, 1)],
            &index,
            |_, _, _| None,
            |_, _| false,
        );
        assert!(outcome.found.is_empty());
        assert_eq!((outcome.left.len(), outcome.ambiguous), (1, 1));
    }

    #[test]
    fn two_records_of_one_kind_on_one_key_both_stay_aside() {
        let mut index = Index::default();
        index.add(chest(1), 1027);
        let records = vec![
            pending(0, Anchor::Placed(chest(1)), 1, 1),
            pending(0, Anchor::Placed(chest(1)), 1, 2),
            pending(1, Anchor::Placed(chest(1)), 1, 3),
        ];
        let outcome = resolve(records, &index, |_, _, _| None, |_, _| false);
        assert_eq!(outcome.ambiguous, 2);
        assert_eq!(outcome.found.len(), 1, "another kind on the key is fine");
        assert_eq!(outcome.found[0].0, 1);
    }

    #[test]
    fn an_item_is_found_by_its_owner_link_and_slot_only_while_it_holds_its_item() {
        let mut index = Index::default();
        index.add(chest(1), 1046);
        let held = |root, link, slot| match (root, link, slot) {
            (1046, 0, 3) => Some((1047, 77)),
            (1046, 0, 4) => Some((1048, 78)),
            _ => None,
        };
        let item = |slot, check| {
            pending(
                0,
                Anchor::Held {
                    root: chest(1),
                    link: 0,
                    slot,
                },
                check,
                slot,
            )
        };
        let outcome = resolve(
            vec![item(3, 77), item(4, 77), item(5, 77)],
            &index,
            held,
            |_, _| false,
        );
        assert_eq!(outcome.found.len(), 1);
        assert_eq!(outcome.found[0].1, 1047);
        assert_eq!(outcome.left.len(), 2, "another item, an empty slot");
    }

    #[test]
    fn session_ids_and_missing_entities_go_aside_with_their_place_kept() {
        let records = vec![
            pending(0, Anchor::Session(1043), 1, 1),
            pending(0, Anchor::Placed(chest(9)), 1, 2),
        ];
        let outcome = resolve(
            records.clone(),
            &Index::default(),
            |_, _, _| None,
            |_, _| false,
        );
        let aside: Vec<Pending> = records
            .into_iter()
            .map(|r| Pending { aside: true, ..r })
            .collect();
        assert_eq!(outcome.left, aside);
        assert_eq!((outcome.earlier, outcome.missing), (1, 1));
    }

    #[test]
    fn a_record_aside_never_matches_again_even_when_its_place_holds_one_entity() {
        // A chest of two on one place is gone, or a new chest stands where a
        // missing one stood: the place now names one entity, maybe not its
        // owner's.
        let mut index = Index::default();
        index.add(chest(1), 1027);
        let record = Pending {
            aside: true,
            ..pending(0, Anchor::Placed(chest(1)), 1, 1)
        };
        let outcome = resolve(vec![record.clone()], &index, |_, _, _| None, |_, _| false);
        assert!(outcome.found.is_empty());
        assert_eq!(outcome.left, [record]);
        assert_eq!(outcome.earlier, 1);
    }

    #[test]
    fn a_record_aside_does_not_make_a_live_record_on_its_place_ambiguous() {
        let mut index = Index::default();
        index.add(chest(1), 1027);
        let old = Pending {
            aside: true,
            ..pending(0, Anchor::Placed(chest(1)), 1, 1)
        };
        let now = pending(0, Anchor::Placed(chest(1)), 1, 2);
        let outcome = resolve(vec![old, now], &index, |_, _, _| None, |_, _| false);
        assert_eq!(outcome.found.len(), 1);
        assert_eq!(outcome.found[0].2.value[0], 2);
    }

    #[test]
    fn a_value_set_before_the_records_were_found_wins_and_the_old_one_stays_aside() {
        let mut index = Index::default();
        index.add(chest(1), 1027);
        let outcome = resolve(
            vec![pending(4, Anchor::Placed(chest(1)), 1, 1)],
            &index,
            |_, _, _| None,
            |kind, entity| (kind, entity) == (4, 1027),
        );
        assert!(outcome.found.is_empty());
        assert_eq!((outcome.left.len(), outcome.replaced), (1, 1));
    }
}

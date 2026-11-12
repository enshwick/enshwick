//! A world's `attachments.bin`: the magic, a version byte, then records of a
//! kind id, a flags byte, an anchor, the check and the value.
//!
//! Version 1 kept each record under its entity id instead of an anchor. A
//! load renumbers entities, so those ids name nothing for certain: they are
//! read as [`Anchor::Session`] records aside, never attached.

use super::anchor::Anchor;

pub(crate) const MAGIC: &[u8; 4] = b"EWID";
/// The version that keeps entity ids.
const VERSION_IDS: u8 = 1;
/// The version that keeps anchors.
pub(crate) const VERSION: u8 = 2;
/// The longest kind id: a mod id, a colon and a name of up to 64 each.
const MAX_ID: usize = 129;
/// The flag of a record aside, which never attaches again.
const ASIDE: u8 = 1;

/// One record as the file holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved<'a> {
    pub(crate) id: &'a str,
    pub(crate) anchor: Anchor,
    pub(crate) check: u32,
    pub(crate) value: &'a [u8],
    pub(crate) aside: bool,
}

/// Appends one record.
fn encode(out: &mut Vec<u8>, record: &Saved<'_>) {
    out.push(u8::try_from(record.id.len()).unwrap_or(u8::MAX));
    out.extend_from_slice(record.id.as_bytes());
    out.push(if record.aside { ASIDE } else { 0 });
    record.anchor.encode(out);
    out.extend_from_slice(&record.check.to_le_bytes());
    out.extend_from_slice(&u16::try_from(record.value.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(record.value);
}

/// The kind id `bytes` start with, and the bytes after it.
fn id(bytes: &[u8]) -> Option<(&str, &[u8])> {
    let (&len, rest) = bytes.split_first()?;
    let len = usize::from(len);
    if len > MAX_ID {
        return None;
    }
    let (id, rest) = rest.split_at_checked(len)?;
    Some((core::str::from_utf8(id).ok()?, rest))
}

/// The check, the value and the bytes after them.
fn tail(bytes: &[u8]) -> Option<(u32, &[u8], &[u8])> {
    let (check, rest) = bytes.split_first_chunk::<4>()?;
    let (size, rest) = rest.split_first_chunk::<2>()?;
    let (value, rest) = rest.split_at_checked(usize::from(u16::from_le_bytes(*size)))?;
    Some((u32::from_le_bytes(*check), value, rest))
}

fn record(bytes: &[u8]) -> Option<(Saved<'_>, &[u8])> {
    let (id, rest) = id(bytes)?;
    let (&flags, rest) = rest.split_first()?;
    if flags & !ASIDE != 0 {
        return None;
    }
    let (anchor, rest) = Anchor::decode(rest)?;
    let (check, value, rest) = tail(rest)?;
    Some((
        Saved {
            id,
            anchor,
            check,
            value,
            aside: flags & ASIDE != 0,
        },
        rest,
    ))
}

fn record_with_id(bytes: &[u8]) -> Option<(Saved<'_>, &[u8])> {
    let (id, rest) = id(bytes)?;
    let (entity, rest) = rest.split_first_chunk::<4>()?;
    let (check, value, rest) = tail(rest)?;
    Some((
        Saved {
            id,
            anchor: Anchor::Session(u32::from_le_bytes(*entity)),
            check,
            value,
            aside: true,
        },
        rest,
    ))
}

/// The records of a file; `None` for a file of another kind, a newer
/// version or a record that does not read, which the loader leaves alone
/// rather than write back without what it could not read.
#[must_use]
pub fn read(bytes: &[u8]) -> Option<Vec<Saved<'_>>> {
    let rest = bytes.strip_prefix(MAGIC.as_slice())?;
    let (&version, mut rest) = rest.split_first()?;
    let next = match version {
        VERSION_IDS => record_with_id,
        VERSION => record,
        _ => return None,
    };
    let mut records = Vec::new();
    while !rest.is_empty() {
        let (saved, after) = next(rest)?;
        if !saved.value.is_empty() {
            records.push(saved);
        }
        rest = after;
    }
    Some(records)
}

/// A file's bytes: the header, then `records` in order.
pub fn write<'a>(records: impl IntoIterator<Item = Saved<'a>>) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.push(VERSION);
    for record in records {
        encode(&mut bytes, &record);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::super::anchor::Placed;
    use super::*;

    fn saved<'a>(id: &'a str, anchor: Anchor, value: &'a [u8]) -> Saved<'a> {
        Saved {
            id,
            anchor,
            check: 7,
            value,
            aside: false,
        }
    }

    #[test]
    fn records_read_back_as_written() {
        let chest = Placed::new([1; 16], [2 << 32; 3]);
        let records = vec![
            saved("a:charges", Anchor::Placed(chest), &[3]),
            Saved {
                aside: true,
                ..saved(
                    "b:name",
                    Anchor::Held {
                        root: chest,
                        link: 1,
                        slot: 4,
                    },
                    b"Oswald",
                )
            },
            saved("gone:kind", Anchor::Session(900), &[5]),
        ];
        let bytes = write(records.clone());
        assert_eq!(read(&bytes), Some(records));
    }

    #[test]
    fn a_file_that_does_not_read_to_its_end_is_left_alone() {
        let chest = Placed::new([1; 16], [2 << 32; 3]);
        let bytes = write([
            saved("a:charges", Anchor::Placed(chest), &[3]),
            saved("a:charges", Anchor::Session(5), &[4]),
        ]);
        assert_eq!(read(&bytes[..bytes.len() - 2]), None, "a cut record");
        let mut flagged = bytes.clone();
        flagged[1 + 4 + 1 + "a:charges".len()] = 0x80;
        assert_eq!(read(&flagged), None, "a flag this loader does not know");
        assert_eq!(read(b"EWID\x09"), None, "a newer version");
        assert_eq!(read(b"nope"), None);
        assert_eq!(read(b"EWID\x02"), Some(Vec::new()), "an empty file");
    }

    #[test]
    fn a_file_of_entity_ids_reads_as_session_records_aside() {
        let mut bytes = MAGIC.to_vec();
        bytes.push(VERSION_IDS);
        for (id, entity, item, value) in [
            ("a:charges", 900u32, 7u32, &[3u8][..]),
            ("b:owner", 1043, 0x5e01, b"Ann"),
            ("a:charges", 901, 7, &[]),
        ] {
            bytes.push(u8::try_from(id.len()).unwrap());
            bytes.extend_from_slice(id.as_bytes());
            bytes.extend_from_slice(&entity.to_le_bytes());
            bytes.extend_from_slice(&item.to_le_bytes());
            bytes.extend_from_slice(&u16::try_from(value.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(value);
        }
        let records = read(&bytes).unwrap();
        assert_eq!(
            records,
            [
                Saved {
                    id: "a:charges",
                    anchor: Anchor::Session(900),
                    check: 7,
                    value: &[3],
                    aside: true,
                },
                Saved {
                    id: "b:owner",
                    anchor: Anchor::Session(1043),
                    check: 0x5e01,
                    value: b"Ann",
                    aside: true,
                },
            ]
        );
        let again = write(records.clone());
        assert_eq!(read(&again), Some(records), "written back as version 2");
    }
}

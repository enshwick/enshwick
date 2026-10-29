//! The bytes of mod item data, apart from the game so they are tested and
//! fuzzed everywhere: the records the server sends a client, the world's
//! file and the anchors that file keeps records under.

#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game finds anchored entities")
)]
pub(crate) mod anchor;
pub mod file;

/// The longest kind id: a mod id, a colon and a name of up to 64 each.
const MAX_ID: usize = 129;

/// Appends one record of a message to a client: the kind id, the entity,
/// the item and the value.
pub fn encode(out: &mut Vec<u8>, id: &str, pide: u32, item: u32, value: &[u8]) {
    out.push(u8::try_from(id.len()).unwrap_or(u8::MAX));
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(&pide.to_le_bytes());
    out.extend_from_slice(&item.to_le_bytes());
    out.extend_from_slice(&u16::try_from(value.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(value);
}

/// The records in a message from the server, each `(id, pide, item,
/// value)`; stops at the first that does not read.
#[must_use]
pub fn decode(mut bytes: &[u8]) -> Vec<(&str, u32, u32, &[u8])> {
    let mut records = Vec::new();
    while let Some((&len, rest)) = bytes.split_first() {
        let len = usize::from(len);
        if len > MAX_ID || rest.len() < len + 10 {
            break;
        }
        let (id, rest) = rest.split_at(len);
        let Ok(id) = core::str::from_utf8(id) else {
            break;
        };
        let word =
            |at: usize| u32::from_le_bytes([rest[at], rest[at + 1], rest[at + 2], rest[at + 3]]);
        let (pide, item) = (word(0), word(4));
        let size = usize::from(u16::from_le_bytes([rest[8], rest[9]]));
        let Some(value) = rest.get(10..10 + size) else {
            break;
        };
        records.push((id, pide, item, value));
        bytes = &rest[10 + size..];
    }
    records
}

use super::menu::file;
use super::*;

#[test]
fn filing_marks_a_free_slot_and_overwrites_a_used_one() {
    unsafe extern "system" fn find(_map: *mut u8, key: *const u32) -> u64 {
        // SAFETY: `file` passes a pointer to its own `u32` item.
        u64::from(unsafe { key.read() } & 127)
    }
    let mut bits = [0u64; 2];
    let mut keys = [0u32; 128];
    let mut values = [0u32; 128];
    let mut table = [0u8; 0x60];
    table[0x08..0x10].copy_from_slice(&(bits.as_mut_ptr() as u64).to_le_bytes());
    table[0x10..0x18].copy_from_slice(&128u64.to_le_bytes());
    table[0x28..0x30].copy_from_slice(&(keys.as_mut_ptr() as u64).to_le_bytes());
    table[0x40..0x48].copy_from_slice(&(values.as_mut_ptr() as u64).to_le_bytes());
    let map = ItemMap {
        find_slot: find,
        loaded: 0x20,
        at: 0,
        bits: 0x08,
        capacity: 0x10,
        keys: 0x28,
        values: 0x40,
        count: 0x54,
    };
    // SAFETY: `table` is laid out at `map`'s offsets over the test's own
    // 128-slot bitset, keys and values, `find` is its slot search, and
    // only this thread uses them.
    assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 5) });
    // SAFETY: `table` is laid out at `map`'s offsets over the test's own
    // 128-slot bitset, keys and values, `find` is its slot search, and
    // only this thread uses them.
    assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 6) });
    assert_eq!(bits[1], 1 << 6);
    assert_eq!((keys[70], values[70]), (70, 6));
    assert_eq!(u32::from_le_bytes(table[0x54..0x58].try_into().unwrap()), 1);
}

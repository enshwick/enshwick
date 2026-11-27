//! FNV-1a, the hash the engine keys names, resource types and ids with,
//! in its 32- and 64-bit forms.

/// The FNV-1a 32 offset basis.
pub const BASIS32: u32 = 0x811c_9dc5;
/// The FNV-1a 32 prime.
pub const PRIME32: u32 = 0x0100_0193;
/// The FNV-1a 64 offset basis.
pub const BASIS64: u64 = 0xcbf2_9ce4_8422_2325;
/// The FNV-1a 64 prime.
pub const PRIME64: u64 = 0x0100_0000_01b3;

/// FNV-1a over 32 bits, the hash the engine keys type names, resource types
/// and GUID-derived ids with.
///
/// # Examples
///
/// ```
/// use enshwick::resources::fnv1a32;
///
/// assert_eq!(fnv1a32(b""), 0x811c_9dc5);
/// assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
/// ```
#[must_use]
pub const fn fnv1a32(bytes: &[u8]) -> u32 {
    fnv1a32_from(BASIS32, bytes)
}

/// FNV-1a 32 over `bytes` from `seed`, each byte taken unsigned: a hash
/// over several pieces in a row passes each piece's result on as the next
/// piece's seed.
#[must_use]
pub const fn fnv1a32_from(seed: u32, bytes: &[u8]) -> u32 {
    let mut hash = seed;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u32).wrapping_mul(PRIME32);
        i += 1;
    }
    hash
}

/// FNV-1a 64 over `bytes` from `seed`, each byte taken unsigned; `seed` is
/// [`BASIS64`] unless the hash's owner starts elsewhere.
#[must_use]
pub const fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = seed;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u64).wrapping_mul(PRIME64);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_hashes_match_the_published_fnv1a_vectors() {
        assert_eq!(fnv1a32(b""), BASIS32);
        assert_eq!(fnv1a32(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
        assert_eq!(fnv1a64(BASIS64, b""), BASIS64);
        assert_eq!(fnv1a64(BASIS64, b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(BASIS64, b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn a_seeded_hash_over_pieces_in_a_row_is_the_hash_of_their_join() {
        assert_eq!(fnv1a32_from(BASIS32, b"foobar"), fnv1a32(b"foobar"));
        assert_eq!(fnv1a32_from(fnv1a32(b"foo"), b"bar"), fnv1a32(b"foobar"));
    }
}

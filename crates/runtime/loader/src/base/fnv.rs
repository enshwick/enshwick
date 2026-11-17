//! FNV-1a 64, the hash the game and the loader's own wire use for names,
//! account keys and checks.

/// The FNV-1a 64 offset basis.
pub(crate) const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// The FNV-1a 64 prime.
pub(crate) const PRIME: u64 = 0x100_0000_01b3;

/// FNV-1a 64 over `bytes` from `seed`, each byte taken unsigned; `seed` is
/// [`BASIS`] unless the hash's owner starts elsewhere.
pub(crate) const fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = seed;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u64).wrapping_mul(PRIME);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_matches_the_published_fnv1a64_vectors() {
        assert_eq!(fnv1a64(BASIS, b""), BASIS);
        assert_eq!(fnv1a64(BASIS, b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(BASIS, b"foobar"), 0x8594_4171_f739_67e8);
    }
}

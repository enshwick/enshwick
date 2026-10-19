//! The game builds the loader knows, and the engine functions it pins on each.
//!
//! Systems resolve by name through reflection and survive updates on their
//! own. Engine helpers that reflection does not describe are pinned here by
//! symbolic id, as SKSE's Address Library does, so mods name them and a game
//! update needs only a new row. Each row also lists systems whose addresses
//! must match reflection, so a row is never applied to the wrong binary.
//! Addresses specific to one mod, such as the call sites it tells apart, live
//! in that mod's own rows.

use enshwick_api::abi::Side;
use enshwick_reflect::Fingerprint;

/// One known game build.
#[derive(Debug)]
pub struct KnownBuild {
    /// For the log, such as `1.0 server (1269579)`.
    pub name: &'static str,
    /// Server or client.
    pub side: Side,
    /// The build's identity.
    pub fingerprint: Fingerprint,
    /// Systems whose reflected entry must equal the pinned address.
    pub checks: &'static [(&'static str, u64)],
    /// Symbolic id and address relative to the image base.
    pub symbols: &'static [(&'static str, u64)],
}

impl KnownBuild {
    /// The address of `name` on this build, relative to the image base.
    #[must_use]
    pub fn symbol(&self, name: &str) -> Option<u64> {
        self.symbols
            .iter()
            .find(|(id, _)| *id == name)
            .map(|&(_, rva)| rva)
    }
}

/// The row for a build, if the loader knows it.
#[must_use]
pub fn find(side: Side, fingerprint: Fingerprint) -> Option<&'static KnownBuild> {
    BUILDS
        .iter()
        .find(|b| b.side == side && b.fingerprint == fingerprint)
}

const fn guid(hex: &[u8; 32]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (digit(hex[2 * i]) << 4) | digit(hex[2 * i + 1]);
        i += 1;
    }
    out
}

#[expect(
    clippy::panic,
    reason = "const evaluation: a malformed literal fails the build"
)]
const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("a fingerprint is lowercase hex"),
    }
}

/// Every build the loader knows.
pub static BUILDS: &[KnownBuild] = &[
    KnownBuild {
        name: "1.0 server (1269579)",
        side: Side::SERVER,
        fingerprint: Fingerprint::new(guid(b"1c129660cd6efc4f9f0b21652d5ff2bf"), 387),
        checks: &[
            ("upgrade_building_range", 0x90a60),
            ("add_buildzones_server", 0xe90e0),
        ],
        symbols: &[
            ("ecs.iterator_init", 0x65_b8c0),
            ("ecs.iterator_next", 0x65_6690),
            ("ecs.attribute_find", 0x98_4940),
            ("ecs.mark_changed", 0x66_6a90),
            ("ecs.component_read", 0x3_46c0),
            ("balancing.zone_size", 0x92_7560),
            ("building.seam_check", 0x93_e210),
            ("building.max_altars", 0x93_1860),
            ("npc.lookup", 0x20_2e80),
            ("npc.lookup_cursor", 0x20_2e30),
            ("npc.move", 0x1e_ca40),
            ("npc.update", 0x20_55f0),
            ("npc.bed_check", 0x1e_e250),
        ],
    },
    KnownBuild {
        name: "1.0 client (1269579)",
        side: Side::CLIENT,
        fingerprint: Fingerprint::new(guid(b"69f0e9573b32c24a8838d3fff550824c"), 410),
        checks: &[("add_buildzones_client", 0x32_88e0)],
        symbols: &[
            ("ecs.iterator_init", 0x9b_ab60),
            ("ecs.iterator_next", 0x9b_5a50),
            ("ecs.attribute_find", 0x121_bea0),
            ("ecs.mark_changed", 0x9c_5df0),
            ("ecs.template_guid", 0x9b_3220),
            ("balancing.zone_size", 0xe7_2e50),
            ("building.seam_check", 0xe8_99f0),
            ("building.max_altars", 0xe7_d070),
            ("net.server_change", 0x107_d200),
            ("npc.lookup_cursor", 0x46_9860),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_unique_and_ids_are_unique_within_a_row() {
        for (i, build) in BUILDS.iter().enumerate() {
            assert!(
                BUILDS[..i]
                    .iter()
                    .all(|b| b.side != build.side || b.fingerprint != build.fingerprint)
            );
            for (j, (id, rva)) in build.symbols.iter().enumerate() {
                assert!(
                    build.symbols[..j].iter().all(|(other, _)| other != id),
                    "{id} twice"
                );
                assert_ne!(*rva, 0, "{id}");
            }
        }
    }

    #[test]
    fn a_known_build_resolves_its_symbols() {
        let server = &BUILDS[0];
        let found = find(Side::SERVER, server.fingerprint).unwrap();
        assert_eq!(found.symbol("npc.lookup"), Some(0x20_2e80));
        assert_eq!(found.symbol("npc.nope"), None);
        assert!(find(Side::CLIENT, server.fingerprint).is_none());
    }
}

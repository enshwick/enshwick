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
    /// Engine constants by symbolic id: offsets and sizes reflection does not
    /// describe.
    pub constants: &'static [(&'static str, u64)],
}

impl KnownBuild {
    /// The engine constant `name` on this build.
    #[must_use]
    pub fn constant(&self, name: &str) -> Option<u64> {
        self.constants
            .iter()
            .find(|(id, _)| *id == name)
            .map(|&(_, value)| value)
    }

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
            ("comfort.base_register", 0x1f_f520),
            ("server.post_step", 0x7b_6d20),
            ("server.set_state", 0x7a_d250),
            ("server.save_step", 0x7b_81b0),
            ("server.player_remove", 0x7a_2300),
            ("ecs.type_index", 0x64_5cc0),
            ("ecs.component_read_raw", 0x64_6b00),
            ("ecs.entity_slot", 0x64_8cb0),
            ("ecs.slot_root", 0x93_5420),
            ("building.spawn_prop", 0x21_dcc0),
            ("building.pickup", 0x22_1590),
            ("resource.open", 0x81_de90),
            ("resource.resolve", 0x81_d9a0),
            ("resource.key_of", 0x81_c190),
            ("chat.post", 0x90_8ce0),
            ("chat.post_return", 0x7b_2d9e),
            ("chat.mark_delivered", 0x94_2740),
            ("label.find", 0x64_6e30),
            ("label.replace", 0x90_9660),
            ("label.add", 0x5f_52a0),
            ("label.rename_return", 0x7b_25e5),
        ],
        constants: &[
            ("ecs.context_row", 8),
            ("ecs.descriptor_entity_records", 0x828),
            ("ecs.entity_record_size", 0x10),
            ("ecs.level_data_offset", 4),
            ("ecs.level_flags", 6),
            ("ecs.attribute_slot", 0xc),
            ("altar.level_attribute", 0x724b_f568),
            ("building.zone_list_entries", 0x10),
            ("building.zone_list_count", 0x18),
            ("building.zone_entry_size", 0x48),
            ("building.zone_entry_shape", 0x44),
            ("building.zone_entry_ignored", 0x46),
            ("building.corner_size", 0x18),
            ("server.entry_state", 0x18),
            ("server.player_slots", 0x1e0),
            ("server.player_slot_size", 0x148),
            ("server.player_slot_count", 16),
            ("server.player_slot_state", 0x0a),
            ("server.player_slot_entity", 0x18),
            ("server.player_slot_key", 0x48),
            ("server.player_slot_name", 0x20),
            ("server.world", 0x1d0),
            ("server.player_infos", 0x1d8),
            ("server.permission_table", 0x498),
            ("server.permission_entry_size", 0xa0),
            ("server.permission_mask", 0x9d),
            ("world.entities", 0xe1_5dc0),
            ("ecs.registry_type_map", 0x30),
            ("ecs.registry_type_map_fallback", 0x88),
            ("ecs.entity_map", 0x148),
            ("ecs.entity_map_capacity", 0x158),
            ("ecs.entity_records", 0x188),
            ("ecs.record_template", 0x28),
            ("ecs.context_entity_world", 0x30),
            ("reflect.type_hash", 0x90),
            ("building.ctx_player", 0x14c),
            ("building.ctx_replay", 0x151),
            ("server.player_slot_machine", 0x04),
            ("server.chat_store", 0x60),
            ("server.custom_strings", 0x58),
            ("chat.store_slots", 0x0c),
            ("chat.store_slot_size", 0x0c),
            ("chat.store_slot_count", 32),
            ("chat.store_tail", 0x1c8),
            ("chat.node_delivered", 0x20),
            ("chat.max_text", 0x400),
            ("chat.reply_type", 4),
            ("label.max_text", 0xff),
            ("label.rename_entity", 0x100),
        ],
    },
    KnownBuild {
        name: "1.0 client (1269579)",
        side: Side::CLIENT,
        fingerprint: Fingerprint::new(guid(b"69f0e9573b32c24a8838d3fff550824c"), 410),
        // The client holds the whole server, run on its `GameServer` thread
        // while it hosts a game: the server's symbols have twins here and its
        // constants hold unchanged.
        checks: &[
            ("add_buildzones_client", 0x32_88e0),
            ("add_buildzones_server", 0x32_8df0),
            ("upgrade_building_range", 0x27_f030),
        ],
        symbols: &[
            ("ecs.iterator_init", 0x9b_ab60),
            ("ecs.iterator_next", 0x9b_5a50),
            ("ecs.attribute_find", 0x121_bea0),
            ("ecs.mark_changed", 0x9c_5df0),
            ("ecs.component_read", 0x21_2f60),
            ("ecs.template_guid", 0x9b_3220),
            ("balancing.zone_size", 0xe7_2e50),
            ("building.seam_check", 0xe8_99f0),
            ("building.max_altars", 0xe7_d070),
            ("npc.lookup", 0x46_98b0),
            ("npc.lookup_cursor", 0x46_9860),
            ("npc.move", 0x44_f040),
            ("npc.update", 0x46_bf80),
            ("npc.bed_check", 0x45_0e20),
            ("comfort.base_register", 0x46_6040),
            ("server.post_step", 0xe4_c620),
            ("server.set_state", 0xe4_2fe0),
            ("server.save_step", 0xe4_d9d0),
            ("server.player_remove", 0xe3_84d0),
            ("ecs.type_index", 0x9a_5080),
            ("ecs.component_read_raw", 0x9a_5ec0),
            ("ecs.entity_slot", 0x9a_8070),
            ("ecs.slot_root", 0xe8_0c10),
            ("building.spawn_prop", 0x48_5a80),
            ("building.pickup", 0x48_9300),
            ("net.server_change", 0x107_d200),
            ("resource.open", 0x106_34f0),
            ("resource.resolve", 0x106_3000),
            ("resource.key_of", 0x106_1800),
            ("text.lookup", 0x11f_8510),
            ("text.lookup_english", 0x11f_80b0),
            ("chat.post", 0xe5_46e0),
            ("chat.post_return", 0xe4_86cf),
            ("chat.mark_delivered", 0xe8_df20),
            ("label.find", 0x9a_61f0),
            ("label.replace", 0xe5_5060),
            ("label.add", 0x95_49a0),
            ("label.rename_return", 0xe4_7fb2),
        ],
        constants: &[
            ("text.pool_language", 0xd8),
            ("ecs.context_row", 8),
            ("ecs.descriptor_entity_records", 0x828),
            ("ecs.entity_record_size", 0x10),
            ("ecs.level_data_offset", 4),
            ("ecs.level_flags", 6),
            ("ecs.attribute_slot", 0xc),
            ("altar.level_attribute", 0x724b_f568),
            ("building.zone_list_entries", 0x10),
            ("building.zone_list_count", 0x18),
            ("building.zone_entry_size", 0x48),
            ("building.zone_entry_shape", 0x44),
            ("building.zone_entry_ignored", 0x46),
            ("building.corner_size", 0x18),
            ("server.entry_state", 0x18),
            ("server.player_slots", 0x1e0),
            ("server.player_slot_size", 0x148),
            ("server.player_slot_count", 16),
            ("server.player_slot_state", 0x0a),
            ("server.player_slot_entity", 0x18),
            ("server.player_slot_key", 0x48),
            ("server.player_slot_name", 0x20),
            ("server.world", 0x1d0),
            ("server.player_infos", 0x1d8),
            ("server.permission_table", 0x498),
            ("server.permission_entry_size", 0xa0),
            ("server.permission_mask", 0x9d),
            ("world.entities", 0xe1_5dc0),
            ("ecs.registry_type_map", 0x30),
            ("ecs.registry_type_map_fallback", 0x88),
            ("ecs.entity_map", 0x148),
            ("ecs.entity_map_capacity", 0x158),
            ("ecs.entity_records", 0x188),
            ("ecs.record_template", 0x28),
            ("ecs.context_entity_world", 0x30),
            ("reflect.type_hash", 0x90),
            ("building.ctx_player", 0x14c),
            ("building.ctx_replay", 0x151),
            ("server.player_slot_machine", 0x04),
            ("server.chat_store", 0x60),
            ("server.custom_strings", 0x58),
            ("chat.store_slots", 0x0c),
            ("chat.store_slot_size", 0x0c),
            ("chat.store_slot_count", 32),
            ("chat.store_tail", 0x1c8),
            ("chat.node_delivered", 0x20),
            ("chat.max_text", 0x400),
            ("chat.reply_type", 4),
            ("label.max_text", 0xff),
            ("label.rename_entity", 0x100),
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

    #[test]
    fn the_client_row_carries_every_server_constant_and_engine_symbol() {
        let (server, client) = (&BUILDS[0], &BUILDS[1]);
        for &(id, value) in server.constants {
            assert_eq!(client.constant(id), Some(value), "{id}");
        }
        for &(id, _) in server.symbols {
            assert!(client.symbol(id).is_some(), "{id}");
        }
        assert_eq!(client.symbol("npc.lookup"), Some(0x46_98b0));
    }
}

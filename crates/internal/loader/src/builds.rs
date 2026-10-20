//! The game builds the loader knows, and the engine functions it pins on each.
//!
//! Systems resolve by name through reflection and survive updates on their
//! own. Engine helpers that reflection does not describe are pinned here by
//! symbolic id, as SKSE's Address Library does, so mods name them and a game
//! update needs only a new row. Each row also lists systems whose addresses
//! must match reflection, so a row is never applied to the wrong binary.
//! Addresses specific to one mod, such as the call sites it tells apart, live
//! in that mod's own rows.

use enshwick_loader_api::abi::Side;
use enshwick_loader_reflect::Fingerprint;

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
            ("inventory_actions", 0x19_f540),
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
            ("skill.node_unlocked", 0x93_fe30),
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
            ("ecs.row_entity", 0x65_3a80),
            ("inventory.actions_row", 0x19_f5f0),
            ("inventory.view_fill", 0x19_f56f),
            ("inventory.skip_entity", 0x131_6740),
            ("inventory.desc_zero", 0x6_a430),
            ("inventory.actor_flag", 0x1b_0780),
            ("inventory.txn_new", 0x3_aba0),
            ("inventory.txn_init", 0x1b_2bf0),
            ("inventory.txn_commit", 0x1a_91f0),
            ("inventory.txn_rollback", 0x1b_1c60),
            ("inventory.txn_drop", 0x3_acf0),
            ("inventory.give", 0x1a_b8e0),
            ("loot.roll", 0x16_6970),
            ("loot.roll_one", 0x16_6cd0),
            ("inventory.take", 0x1a_a720),
            ("inventory.destroy_slot", 0x1a_c130),
            ("inventory.move_slot", 0x1a_e5f0),
            ("inventory.move_amount", 0x1a_e540),
            ("inventory.move_all", 0x1a_d940),
            ("inventory.resend", 0x23_8450),
            ("item.info", 0x92_ef80),
            ("skill.fly_mode_gate_return", 0xc_4875),
            ("experience.need_for_level", 0x93_22c0),
            ("experience.level_cap", 0x93_3f30),
            ("access.offer_test", 0x9_e270),
            ("access.transfer", 0x19_81f0),
            ("access.sort", 0x19_8cf0),
            ("access.base_stock", 0x20_1800),
            ("access.dismantle_method", 0x9_67f0),
            ("survivors.spawn_return", 0x4_05cb),
            ("survivors.unique_return_a", 0x1f_4929),
            ("survivors.unique_return_b", 0x1f_499d),
            ("survivors.bed_return", 0x1f_29ff),
            ("survivors.bed_init_return", 0x1f_2d76),
            ("survivors.cursor_return_a", 0x9_9fe8),
            ("survivors.cursor_return_b", 0x9_9ff3),
            ("survivors.assign_return_a", 0xb_f541),
            ("survivors.assign_return_b", 0xb_f5e4),
            ("survivors.roaming_return", 0x1f_24f8),
            ("survivors.spawn", 0x3_e170),
            ("survivors.registry_load", 0x95_f900),
            ("survivors.map_find_slot", 0x94_4290),
            ("ecs.event_merge", 0x6b_8d20),
            ("ecs.entities_with", 0x64_7e10),
            ("ecs.destroy_request", 0x5f_4b50),
            ("server.save_request", 0x7a_db60),
            ("save.path", 0x96_b590),
            ("ecs.component_write", 0x64_6840),
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
            // The first 8 code bytes at `inventory.actions_row` and
            // `inventory.view_fill`, little-endian: the loop head, and the
            // view fill that names `rbp - 0x60` and `r13`.
            ("inventory.row_bytes", 0x7095_8d48_b065_8b4c),
            ("inventory.view_fill_bytes", 0xe8cd_8b49_a055_8d48),
            ("inventory.view_frame", 0x60),
            ("inventory.view_owners", 0x178),
            ("inventory.view_actors", 0x118),
            ("inventory.view_replication", 0xf8),
            ("inventory.view_item_registry", 0xd8),
            ("inventory.desc_size", 0xf0),
            ("inventory.desc_context", 0x08),
            ("inventory.desc_actor", 0xc8),
            ("inventory.desc_other", 0xcc),
            // Descriptor fields copied from the view: `desc << 32 | view`.
            ("inventory.copy_item_registry", 0x10 << 32 | 0xd8),
            ("inventory.copy_a8", 0x18 << 32 | 0xa8),
            ("inventory.copy_setup", 0x28 << 32 | 0x160),
            ("inventory.copy_slots", 0x30 << 32 | 0x148),
            ("inventory.copy_slot_hints", 0x38 << 32 | 0x190),
            ("inventory.copy_equipment", 0x40 << 32 | 0x168),
            ("inventory.copy_owners", 0x48 << 32 | 0x178),
            ("inventory.copy_empty_state", 0x58 << 32 | 0x158),
            ("inventory.copy_transform", 0x78 << 32 | 0x110),
            ("inventory.copy_100", 0x80 << 32 | 0x100),
            ("inventory.copy_150", 0x88 << 32 | 0x150),
            ("inventory.copy_pickup_events", 0xa0 << 32 | 0x1a8),
            ("inventory.copy_full_events", 0xa8 << 32 | 0x1a0),
            ("inventory.copy_198", 0xb8 << 32 | 0x198),
            ("inventory.copy_discovered", 0xc0 << 32 | 0xf0),
            ("inventory.txn_size", 0x71b0),
            ("loot.txn_context", 0),
            ("loot.txn_entity", 0xe0),
            ("loot.request_node", 0),
            ("loot.request_labels", 4),
            ("experience.gained", 0x1d7d_d102),
            ("experience.progress", 0x8dcd_daf4),
            ("experience.need", 0x0472_3f62),
            ("access.state_input", 0x10),
            ("access.state_owner_index", 0x178),
            ("access.transfer_source", 0x0c),
            ("access.transfer_target", 0x10),
            ("access.transfer_source_slot", 0x14),
            ("access.transfer_target_slot", 0x1c),
            ("access.sort_target", 0x74),
            ("access.object_id", 0x10),
            ("access.object_kind", 0x18),
            ("access.deposit_target", 0x100),
            ("access.deposit_version", 0x108),
            ("access.consumed_deposit", 0x48),
            ("access.upgrade_target", 0x10c),
            ("access.upgrade_version", 0x144),
            ("access.consumed_upgrade", 0x4c),
            ("survivors.unique_context", 0x38),
            ("survivors.unique_saved_state", 0x40),
            ("survivors.unique_transform", 0x48),
            ("survivors.bed_context", 0x40),
            ("survivors.bed_transform", 0x48),
            ("survivors.bed_npc_common_type", 0xa0),
            ("survivors.npc_common_bed", 0x48),
            ("survivors.assign_context", 0x30),
            ("survivors.assign_targets", 0x40),
            ("survivors.assign_bed", 0x85c),
            ("survivors.roaming_context", 0x40),
            ("survivors.spawn_target", 0x18),
            ("survivors.move_survivor", 0x450),
            ("survivors.npc_transform", 0x168),
            ("survivors.npc_saved_state", 0x1e0),
            ("survivors.npc_common", 0x300),
            ("survivors.npc_state", 0x450),
            ("survivors.spawn_template", 0x08),
            ("survivors.spawn_used_item", 0x98),
            ("survivors.spawn_ammo_item", 0xac),
            ("survivors.spawn_consume", 0xe0),
            ("survivors.registry_loaded", 0x20),
            ("survivors.registry_item_map", 0xa8),
            ("survivors.map_bits", 0x08),
            ("survivors.map_capacity", 0x10),
            ("survivors.map_keys", 0x28),
            ("survivors.map_values", 0x40),
            ("survivors.map_count", 0x54),
            ("ecs.event_streams", 0xeb_9ef0),
            ("ecs.event_stream_count", 0xeb_9ef8),
            ("ecs.event_stream_size", 0x18),
            ("ecs.event_stream_used", 0x10),
            ("ecs.event_record_size", 6),
            ("ecs.event_record_header", 8),
            ("ecs.event_record_align", 8),
            ("ecs.entities_entry_size", 0x40),
            ("ecs.entities_entry_transform", 8),
            ("ecs.command_buffers", 0xeb_9f08),
            ("server.save_system", 0x20),
            ("server.save_blocked", 0x18aa),
            ("save.flags", 0x70),
            ("save.phase", 0x78),
            ("save.path_data", 8),
            ("save.path_lengths", 0x830),
            ("save.path_parts", 4),
            // Attribute sets by the id of their first attribute, which a set
            // keeps, and the slots of the attributes read from them.
            ("ecs.attribute_count", 7),
            ("attribute.health", 0x8eb8_4995),
            ("attribute.health_max_slot", 7),
            ("attribute.stamina", 0x04b6_aa8b),
            ("attribute.stamina_max_slot", 2),
            ("attribute.mana", 0x6aa2_574e),
            ("attribute.mana_max_slot", 1),
            ("attribute.level", 0x724b_f568),
            ("attribute.experience", 0x8dcd_daf4),
            ("attribute.experience_needed_slot", 2),
            // The entity world's change tracker, which writes outside a
            // system mark against.
            ("ecs.change_tracker", 0xeb_c030),
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
            ("inventory_actions", 0x3f_d610),
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
            ("skill.node_unlocked", 0xe8_b610),
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
            ("ecs.row_entity", 0x9b_2e40),
            ("inventory.actions_row", 0x3f_d6c0),
            ("inventory.view_fill", 0x3f_d63f),
            ("inventory.skip_entity", 0x1d8_cda0),
            ("inventory.desc_zero", 0x25_7ab0),
            ("inventory.actor_flag", 0x40_fbb0),
            ("inventory.txn_new", 0x21_dd00),
            ("inventory.txn_init", 0x41_1df0),
            ("inventory.txn_commit", 0x40_8b50),
            ("inventory.txn_rollback", 0x41_0e60),
            ("inventory.txn_drop", 0x21_de50),
            ("inventory.give", 0x40_b170),
            ("loot.roll", 0x3c_2200),
            ("loot.roll_one", 0x3c_2560),
            ("inventory.take", 0x40_a080),
            ("inventory.destroy_slot", 0x40_b920),
            ("inventory.move_slot", 0x40_dde0),
            ("inventory.move_amount", 0x40_dd30),
            ("inventory.move_all", 0x40_d130),
            ("inventory.resend", 0x49_ee40),
            ("item.info", 0xe7_a870),
            ("skill.fly_mode_gate_return", 0x2d_8295),
            ("experience.need_for_level", 0xe7_dad0),
            ("experience.level_cap", 0xe7_f720),
            ("access.offer_test", 0x2a_0850),
            ("access.transfer", 0x3f_62e0),
            ("access.sort", 0x3f_6de0),
            ("access.base_stock", 0x46_82e0),
            ("access.base_stock_ui_return", 0x2b_884b),
            ("access.dismantle_method", 0x28_a170),
            ("survivors.cursor_return", 0x29_63a8),
            ("survivors.spawn_return", 0x22_368b),
            ("survivors.unique_return_a", 0x45_b299),
            ("survivors.unique_return_b", 0x45_b30d),
            ("survivors.bed_return", 0x45_744f),
            ("survivors.bed_init_return", 0x45_77c6),
            ("survivors.cursor_return_a", 0x29_63a8),
            ("survivors.cursor_return_b", 0x29_63b3),
            ("survivors.assign_return_a", 0x2d_2f61),
            ("survivors.assign_return_b", 0x2d_3004),
            ("survivors.roaming_return", 0x45_6f48),
            ("survivors.spawn", 0x22_1230),
            ("survivors.registry_load", 0xea_a500),
            ("survivors.map_find_slot", 0xe8_fa70),
            ("ecs.event_merge", 0xa1_77b0),
            ("ecs.entities_with", 0x9a_71d0),
            ("ecs.destroy_request", 0x95_4250),
            ("server.save_request", 0xe4_38f0),
            ("save.path", 0x102_2160),
            ("ecs.component_write", 0x9a_5c00),
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
            // The first 8 code bytes at `inventory.actions_row` and
            // `inventory.view_fill`, little-endian: the loop head, and the
            // view fill that names `rbp - 0x60` and `r13`.
            ("inventory.row_bytes", 0x7095_8d48_b065_8b4c),
            ("inventory.view_fill_bytes", 0xe8cd_8b49_a055_8d48),
            ("inventory.view_frame", 0x60),
            ("inventory.view_owners", 0x178),
            ("inventory.view_actors", 0x118),
            ("inventory.view_replication", 0xf8),
            ("inventory.view_item_registry", 0xd8),
            ("inventory.desc_size", 0xf0),
            ("inventory.desc_context", 0x08),
            ("inventory.desc_actor", 0xc8),
            ("inventory.desc_other", 0xcc),
            // Descriptor fields copied from the view: `desc << 32 | view`.
            ("inventory.copy_item_registry", 0x10 << 32 | 0xd8),
            ("inventory.copy_a8", 0x18 << 32 | 0xa8),
            ("inventory.copy_setup", 0x28 << 32 | 0x160),
            ("inventory.copy_slots", 0x30 << 32 | 0x148),
            ("inventory.copy_slot_hints", 0x38 << 32 | 0x190),
            ("inventory.copy_equipment", 0x40 << 32 | 0x168),
            ("inventory.copy_owners", 0x48 << 32 | 0x178),
            ("inventory.copy_empty_state", 0x58 << 32 | 0x158),
            ("inventory.copy_transform", 0x78 << 32 | 0x110),
            ("inventory.copy_100", 0x80 << 32 | 0x100),
            ("inventory.copy_150", 0x88 << 32 | 0x150),
            ("inventory.copy_pickup_events", 0xa0 << 32 | 0x1a8),
            ("inventory.copy_full_events", 0xa8 << 32 | 0x1a0),
            ("inventory.copy_198", 0xb8 << 32 | 0x198),
            ("inventory.copy_discovered", 0xc0 << 32 | 0xf0),
            ("inventory.txn_size", 0x71b0),
            ("loot.txn_context", 0),
            ("loot.txn_entity", 0xe0),
            ("loot.request_node", 0),
            ("loot.request_labels", 4),
            ("experience.gained", 0x1d7d_d102),
            ("experience.progress", 0x8dcd_daf4),
            ("experience.need", 0x0472_3f62),
            ("access.state_input", 0x10),
            ("access.state_owner_index", 0x178),
            ("access.transfer_source", 0x0c),
            ("access.transfer_target", 0x10),
            ("access.transfer_source_slot", 0x14),
            ("access.transfer_target_slot", 0x1c),
            ("access.sort_target", 0x74),
            ("access.object_id", 0x10),
            ("access.object_kind", 0x18),
            ("access.deposit_target", 0x100),
            ("access.deposit_version", 0x108),
            ("access.consumed_deposit", 0x48),
            ("access.upgrade_target", 0x10c),
            ("access.upgrade_version", 0x144),
            ("access.consumed_upgrade", 0x4c),
            ("survivors.unique_context", 0x38),
            ("survivors.unique_saved_state", 0x40),
            ("survivors.unique_transform", 0x48),
            ("survivors.bed_context", 0x40),
            ("survivors.bed_transform", 0x48),
            ("survivors.bed_npc_common_type", 0xa0),
            ("survivors.npc_common_bed", 0x48),
            ("survivors.assign_context", 0x30),
            ("survivors.assign_targets", 0x40),
            ("survivors.assign_bed", 0x85c),
            ("survivors.roaming_context", 0x40),
            ("survivors.spawn_target", 0x18),
            ("survivors.move_survivor", 0x450),
            ("survivors.npc_transform", 0x168),
            ("survivors.npc_saved_state", 0x1e0),
            ("survivors.npc_common", 0x300),
            ("survivors.npc_state", 0x450),
            ("survivors.spawn_template", 0x08),
            ("survivors.spawn_used_item", 0x98),
            ("survivors.spawn_ammo_item", 0xac),
            ("survivors.spawn_consume", 0xe0),
            ("survivors.registry_loaded", 0x20),
            ("survivors.registry_item_map", 0xa8),
            ("survivors.map_bits", 0x08),
            ("survivors.map_capacity", 0x10),
            ("survivors.map_keys", 0x28),
            ("survivors.map_values", 0x40),
            ("survivors.map_count", 0x54),
            ("survivors.state_status", 0x14),
            ("survivors.ui_map_bits", 0x46_3e98),
            ("survivors.ui_map_capacity", 0x46_3ea0),
            ("survivors.ui_map_keys", 0x46_3eb8),
            ("survivors.ui_map_values", 0x46_3ed0),
            ("survivors.ui_map_count", 0x46_3ee4),
            ("survivors.ui_record_size", 0x928),
            ("survivors.ui_record_status", 0x880),
            ("ecs.event_streams", 0xeb_9ef0),
            ("ecs.event_stream_count", 0xeb_9ef8),
            ("ecs.event_stream_size", 0x18),
            ("ecs.event_stream_used", 0x10),
            ("ecs.event_record_size", 6),
            ("ecs.event_record_header", 8),
            ("ecs.event_record_align", 8),
            ("ecs.entities_entry_size", 0x40),
            ("ecs.entities_entry_transform", 8),
            ("ecs.command_buffers", 0xeb_9f08),
            ("server.save_system", 0x20),
            ("server.save_blocked", 0x18aa),
            ("save.flags", 0x70),
            ("save.phase", 0x78),
            ("save.path_data", 8),
            ("save.path_lengths", 0x830),
            ("save.path_parts", 4),
            // Attribute sets by the id of their first attribute, which a set
            // keeps, and the slots of the attributes read from them.
            ("ecs.attribute_count", 7),
            ("attribute.health", 0x8eb8_4995),
            ("attribute.health_max_slot", 7),
            ("attribute.stamina", 0x04b6_aa8b),
            ("attribute.stamina_max_slot", 2),
            ("attribute.mana", 0x6aa2_574e),
            ("attribute.mana_max_slot", 1),
            ("attribute.level", 0x724b_f568),
            ("attribute.experience", 0x8dcd_daf4),
            ("attribute.experience_needed_slot", 2),
            // The entity world's change tracker, which writes outside a
            // system mark against.
            ("ecs.change_tracker", 0xeb_c030),
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

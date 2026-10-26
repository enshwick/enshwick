//! The ids the API asks the loader for engine facts by; the loader maps each
//! to the game's names and offsets, so no engine name is compiled into a mod.

/// Declares each id as a constant and lists them all in [`ALL`].
macro_rules! ids {
    ($($name:ident = $id:literal;)*) => {
        $(pub(crate) const $name: &str = $id;)*

        /// Every id, for the loader's test that each one resolves.
        pub const ALL: &[&str] = &[$($id),*];
    };
}

ids! {
    WORLD_TRANSFORM_POSITION = "world_transform.position";
    WORLD_TRANSFORM_ORIENTATION = "world_transform.orientation";
    WORLD_TRANSFORM_SCALE = "world_transform.scale";
    NPC_COMMAND_EVENT = "npc_command_event";
    NPC_COMMAND_KIND = "npc_command_event.kind";
    NPC_COMMAND_NPC = "npc_command_event.npc";
    NPC_COMMAND_TARGET = "npc_command_event.target";
    SET_TOGGLE_STATE_EVENT = "set_toggle_state_event";
    SET_TOGGLE_SENDER = "set_toggle_state_event.sender";
    SET_TOGGLE_SIGNAL = "set_toggle_state_event.signal";
    SET_TOGGLE_STATE = "set_toggle_state_event.state";
    ADD_THREAT_TARGET_EVENT = "add_threat_target_event";
    ADD_THREAT_ATTACKER = "add_threat_target_event.attacker";
    ADD_THREAT_TARGET = "add_threat_target_event.target";
    ADD_THREAT_THREAT = "add_threat_target_event.threat";
    UI_PING_EVENT = "ui_ping_event";
    UI_PING_SENDER = "ui_ping_event.sender";
    UI_PING_POSITION = "ui_ping_event.position";
}
